//! The Shepherd panel: runs (with the pattern launcher and controls), agents, tasks and the monitor of what
//! agents say to each other. It reads `Data` (files) and asks the server for every change.

use crate::pattern_editor::{EditMsg, PatternEditor};
use crate::prism::{Preset, Prism};
use crate::theme::{self, kpi_cell_style, kpi_strip_style};
use crate::theme::{
    card_style, danger_button_style, item_style, pad, primary_button_style, status_color,
    status_glyph, ACCENT, BAD, DIM, FG, LINE, OK, WARN,
};
use crate::{Action, Message};
use iced::widget::canvas::Canvas;
use iced::widget::{
    button, checkbox, column, container, pick_list, row, text, text_editor, text_input,
};
use iced::{Background, Color, Element, Length, Task};
use shepherd_app_core::actions::{self, RunControl};
use shepherd_app_core::bus_message::line as feed_line;
use shepherd_app_core::data::{bus_feed, ensure_task_prefix, task_of_run, task_prompt, Data, Gate};
use shepherd_app_core::pattern_graph::{self, PatternView, PatternView as PatternGraphView};
use shepherd_app_core::shepherd_core::bus::Message as BusMessage;
use shepherd_app_core::shepherd_core::service::patterns_run::PatternRunState;
use shepherd_app_core::snapshot::{Snapshot, Status};
use std::path::PathBuf;

mod snapshots;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Runs,
    Agents,
    Tasks,
    Patterns,
    Monitor,
    Skills,
    Git,
    Events,
    Models,
    Snapshots,
}

/// Seconds an armed (two-click) button waits for its second click.
const ARM_SECS: f32 = 4.0;

/// `HH:MM:SS` (UTC) now, for the notice history.
fn clock_now() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{:02}:{:02}:{:02}", (t / 3600) % 24, (t / 60) % 60, t % 60)
}

/// A button that needs a second click (no pop-ups).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Armed {
    StopRun(String),
    DeleteRun(String),
    CleanRuns,
    #[allow(dead_code)]
    CleanBus,
    CleanBusForce,
    ArchiveDone,
    DeleteTask(u32),
    CancelRun(u32),
    ApproveGate(u32, Gate),
    RejectGate(u32, Gate),
    RestoreSnapshot(String),
    DeleteSnapshot(String),
    /// Deletes an agent's oldest snapshots.
    PruneSnapshots(String),
    /// Deletes a pattern file.
    DeletePattern(String),
}

/// The one-line summary (copy deck, section 7) of what an action was doing; the server's refusal follows it.
fn failure_summary(message: &PanelMsg, armed: Option<&Armed>) -> Option<&'static str> {
    Some(match message {
        PanelMsg::CommSend => "Not sent.",
        PanelMsg::Launch => "Cannot run this pattern yet.",
        PanelMsg::Control(_, RunControl::Stop) => "Not stopped.",
        PanelMsg::Control(_, RunControl::Pause) => "Not paused.",
        PanelMsg::Control(_, RunControl::Resume) => "Not resumed.",
        PanelMsg::AgentSkill(..) => "Not changed.",
        PanelMsg::SpawnAgent => "The agent was not started.",
        PanelMsg::ImportSubmit => "Not imported.",
        PanelMsg::TaskSave => "Not saved.",
        PanelMsg::KillAgent(_) => "Not closed.",
        PanelMsg::ResumeAgents => "Not resumed.",
        PanelMsg::AssignAgentTask(..) => "Not moved.",
        PanelMsg::Confirm => match armed? {
            Armed::StopRun(_) => "Not stopped.",
            Armed::DeleteRun(_)
            | Armed::DeleteTask(_)
            | Armed::DeleteSnapshot(_)
            | Armed::DeletePattern(_) => "Not deleted.",
            Armed::CleanRuns | Armed::CleanBus | Armed::CleanBusForce => "Not cleaned.",
            Armed::ArchiveDone => "Not archived.",
            Armed::CancelRun(_) => "Not cancelled.",
            Armed::ApproveGate(..) => "Not approved.",
            Armed::RejectGate(..) => "Not rejected.",
            Armed::RestoreSnapshot(_) => "Not restored.",
            Armed::PruneSnapshots(_) => "Not pruned.",
        },
        _ => return None,
    })
}

/// A refusal as a notice: the summary, a line break, the server's words as sent.
pub(crate) fn refusal(summary: &str, server: &str) -> String {
    let summary = if summary.is_empty() {
        "Not done."
    } else {
        summary
    };
    format!("{summary}\n{}", server.trim())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    label: String,
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub enum PanelMsg {
    Tab(Tab),
    SelectRun(String),
    SelectAgent(String),
    SelectTask(u32),
    SelectPattern(String),
    PickAndLaunch(String),
    PatternLoaded(Result<PatternView, String>),
    EditPattern(EditMsg),
    OpenLauncher(Option<u32>),
    CloseLauncher,
    PickPattern(Choice),
    Edit(text_editor::Action),
    Launch,
    Control(String, RunControl),
    Arm(Armed),
    Confirm,
    Done(Result<String, String>),
    /// A notice from outside the panel (the window's menus).
    Notify(String),
    SnapSelect(String),
    SnapShown(
        String,
        Result<Box<shepherd_app_core::shepherd_core::snapshot::ShowResult>, String>,
    ),
    SnapCapture(String),
    /// One agent's skill: on, off, or back to what its role gives (`None`). Applies at its next start.
    AgentSkill(String, String, Option<bool>),
    SnapCompare(String),
    SnapCompared(
        String,
        Result<Box<shepherd_app_core::shepherd_core::snapshot::DiffResult>, String>,
    ),
    ShowMonitor(String),
    Feed(Vec<BusMessage>),
    Cfg(crate::config_panel::CfgMsg),
    // Agent spawn form
    OpenSpawnAgent,
    CloseSpawnAgent,
    SpawnAgentRole(Choice),
    SpawnAgentWorkspace(Choice),
    SpawnAgentVendor(Choice),
    SpawnAgentModel(String),
    SpawnAgentTask(String),
    SpawnAgentWorktree(bool),
    SpawnAgent,
    // Agent actions
    KillAgent(String),
    ResumeAgents,
    AssignAgentTask(String, Option<u32>),
    // Task form
    TaskNew,
    RejectReason(String),
    ImportOpen,
    ImportInput(String),
    ImportSubmit,
    ImportCancel,
    TaskEditOpen(u32),
    TaskTitle(String),
    TaskDescription(text_editor::Action),
    TaskCriteria(text_editor::Action),
    TaskSpec(text_editor::Action),
    TaskSave,
    TaskCancel,
    /// Open the agent form for this task.
    RunTaskWithAgent(u32),
    /// The list filter box and its chips.
    Filter(String),
    Chip(usize),
    DismissToast(usize),
    /// What the agents' panes show now.
    Outputs(Vec<(String, String)>),
    /// Opens the editor on a blank pattern.
    NewPattern,
    /// Snapshots: the agent the header's Capture names, and the two chip filters.
    SnapAgent(String),
    SnapFilterAgent(Option<String>),
    SnapFilterReason(Option<String>),
    ActivityWindow(usize),
    /// Opens Events with this topic selected.
    OpenTopic(String),
    CommClose,
    /// Shows or hides the messages of one sender in the communication screen.
    CommToggle(String),
    CommTopic(Option<String>),
    CommText(String),
    CommSend,
}

/// The create / edit form of a task.
pub struct TaskForm {
    /// The task being edited; `None` when it is a new one.
    editing: Option<u32>,
    title: String,
    description: text_editor::Content,
    criteria: text_editor::Content,
    spec: text_editor::Content,
}

impl TaskForm {
    fn new() -> Self {
        Self {
            editing: None,
            title: String::new(),
            description: text_editor::Content::new(),
            criteria: text_editor::Content::new(),
            spec: text_editor::Content::new(),
        }
    }

    fn of(task: &shepherd_app_core::data::TaskRow) -> Self {
        Self {
            editing: Some(task.status.id),
            title: task.status.title.clone(),
            description: text_editor::Content::with_text(&task.issue_body),
            criteria: text_editor::Content::with_text(&shepherd_app_core::data::criteria_text(
                task,
            )),
            spec: text_editor::Content::with_text(&task.spec),
        }
    }
}

pub struct Panel {
    /// One line saying what was being done, shown above the server's refusal (set before every action).
    fail_summary: &'static str,
    pub tab: Tab,
    run: Option<String>,
    agent: Option<String>,
    task: Option<u32>,
    pattern_sel: Option<String>,
    edit: Option<PatternEditor>,
    launching: bool,
    launch_task: Option<u32>,
    pattern: Option<Choice>,
    objective: text_editor::Content,
    armed: Option<Armed>,
    /// The snapshot picked in the Snapshots tab, and what the server said it holds.
    snap: Option<String>,
    snap_detail: Option<(
        String,
        Result<Box<shepherd_app_core::shepherd_core::snapshot::ShowResult>, String>,
    )>,
    /// The shown snapshot compared with the worktree as it is now (asked for, not loaded on its own).
    snap_compare: Option<(
        String,
        Result<Box<shepherd_app_core::shepherd_core::snapshot::DiffResult>, String>,
    )>,
    pub notice: String,
    monitor: Option<String>,
    feed: Vec<BusMessage>,
    cfg: crate::config_panel::ConfigPanel,
    /// The tab the window starts on loads once, when the first data arrives.
    cfg_started: bool,
    // Spawn agent form
    spawning_agent: bool,
    spawn_role: Option<Choice>,
    spawn_workspace: Option<Choice>,
    spawn_vendor: Option<Choice>,
    spawn_model: String,
    spawn_task_text: String,
    spawn_worktree: bool,
    /// The task an agent is being launched for (set by "Run with an agent").
    spawn_task_id: Option<u32>,
    /// The form to create or edit a task, while it is open.
    task_form: Option<TaskForm>,
    /// The issue reference being typed to import a GitHub issue as a task, while that form is open.
    importing: Option<String>,
    /// Why a gate is being rejected (optional): the planner or coder reads it.
    reject_reason: String,
    /// What the list filter holds, and which chip is on (reset when the view changes).
    filter: String,
    chip: usize,
    /// The notice shown as a toast, and the last notice seen (to know when a new one arrives).
    toasts: Vec<crate::ui::Toast>,
    seen_notice: String,
    /// Every notice of this session, newest last, for the status bar history.
    pub history: Vec<(String, String)>,
    /// The graph of the shown run's pattern, for the live topology (cached by pattern name).
    run_graph: Option<(String, shepherd_app_core::pattern_graph::Graph)>,
    /// The last lines each agent's pane shows, by pane id (asked from the API while Agents is open).
    outputs: std::collections::HashMap<String, String>,
    snap_capture_agent: Option<String>,
    snap_agent_filter: Option<String>,
    snap_reason_filter: Option<String>,
    /// When the armed button was armed: it disarms by itself after `ARM_SECS`.
    armed_at: Option<std::time::Instant>,
    /// Activity: 0 = last 5 min, 1 = 30 min, 2 = 2 h.
    activity_window: usize,
    /// The communication screen of a run (opened from Runs), the senders hidden in it, the topic it shows
    /// and what is being typed to the team.
    comm_open: bool,
    comm_hidden: std::collections::HashSet<String>,
    comm_topic: Option<String>,
    comm_text: String,
}

/// What the panel needs from the app to act.
#[derive(Clone)]
pub struct Ctx<'a> {
    pub api: PathBuf,
    pub data: &'a Data,
    pub snapshot: &'a Snapshot,
    /// Animation step of the prism canvases.
    pub frame: u32,
}

impl Default for Panel {
    /// `SHEPHERD_NATIVE_TAB=agents|tasks|monitor` and `SHEPHERD_NATIVE_LAUNCH=1` pick what the window starts
    /// on; they exist to take pictures of each view (`make native-shot`).
    fn default() -> Self {
        let tab = match std::env::var("SHEPHERD_NATIVE_TAB").as_deref() {
            Ok("overview") => Tab::Overview,
            Ok("agents") => Tab::Agents,
            Ok("tasks") => Tab::Tasks,
            Ok("patterns") => Tab::Patterns,
            Ok("monitor") => Tab::Monitor,
            Ok("skills") => Tab::Skills,
            Ok("git") => Tab::Git,
            Ok("events") => Tab::Events,
            Ok("models") => Tab::Models,
            Ok("snapshots") => Tab::Snapshots,
            _ => Tab::Runs,
        };
        Self {
            launching: std::env::var_os("SHEPHERD_NATIVE_LAUNCH").is_some(),
            tab,
            run: None,
            agent: None,
            task: None,
            pattern_sel: None,
            edit: None,
            launch_task: None,
            pattern: None,
            objective: text_editor::Content::new(),
            armed: None,
            snap: None,
            snap_detail: None,
            snap_compare: None,
            notice: String::new(),
            monitor: None,
            feed: Vec::new(),
            cfg: Default::default(),
            cfg_started: false,
            spawning_agent: false,
            spawn_role: None,
            spawn_workspace: None,
            spawn_vendor: None,
            spawn_model: String::new(),
            spawn_task_text: String::new(),
            spawn_worktree: false,
            spawn_task_id: None,
            // picture aid: `SHEPHERD_NATIVE_TASKFORM=new` opens the new-task form (`edit:N` waits for the data)
            task_form: (std::env::var("SHEPHERD_NATIVE_TASKFORM").as_deref() == Ok("new"))
                .then(TaskForm::new),
            importing: None,
            reject_reason: String::new(),
            filter: String::new(),
            chip: 0,
            toasts: Vec::new(),
            seen_notice: String::new(),
            fail_summary: "",
            history: Vec::new(),
            run_graph: None,
            outputs: Default::default(),
            snap_capture_agent: None,
            snap_agent_filter: None,
            snap_reason_filter: None,
            armed_at: None,
            activity_window: 1,
            // picture aid: `SHEPHERD_NATIVE_COMM=1` opens the communication screen
            comm_open: std::env::var_os("SHEPHERD_NATIVE_COMM").is_some(),
            comm_hidden: Default::default(),
            comm_topic: None,
            comm_text: String::new(),
        }
    }
}

fn state_badge(state: PatternRunState) -> (&'static str, Color) {
    match state {
        PatternRunState::Running => ("running", OK),
        PatternRunState::Paused | PatternRunState::Stalled | PatternRunState::Degraded => (
            match state {
                PatternRunState::Paused => "paused",
                PatternRunState::Stalled => "stalled",
                _ => "degraded",
            },
            WARN,
        ),
        PatternRunState::Failed => ("failed", BAD),
        PatternRunState::Done => ("done", DIM),
        PatternRunState::Stopped => ("stopped", DIM),
    }
}

/// What an agent's Skills card says: the skills installed for it, and what was changed for this agent alone.
fn agent_skill_lines(a: &shepherd_app_core::shepherd_core::Agent) -> Vec<String> {
    let mut lines = vec![if a.active_skills.is_empty() {
        "No skills were installed for this agent.".to_owned()
    } else {
        format!("Active: {}", a.active_skills.join(", "))
    }];
    if !a.skills.add.is_empty() {
        lines.push(format!("Added for this agent: {}", a.skills.add.join(", ")));
    }
    if !a.skills.remove.is_empty() {
        lines.push(format!(
            "Removed for this agent: {}",
            a.skills.remove.join(", ")
        ));
    }
    lines
}

/// One row per skill that can be changed for this agent: the active ones can be turned off, the removed ones
/// back on, and anything changed for it can be put back. The server applies it at the agent's next start.
fn agent_skill_buttons<'a>(
    a: &shepherd_app_core::shepherd_core::Agent,
) -> Vec<Element<'a, Message>> {
    let pick = |skill: &str, label: &str, enable: Option<bool>| {
        btn_link(
            label.to_owned(),
            PanelMsg::AgentSkill(a.id.clone(), skill.to_owned(), enable),
        )
    };
    // the first of a row's buttons becomes the switch; "Reset" stays a small link
    let row_of = |skill: &str, enabled: bool, buttons: Vec<Element<'a, Message>>| {
        let id = a.id.clone();
        let name = skill.to_owned();
        let mut r = row![text(skill.to_owned()).size(12.5).width(Length::Fill)]
            .spacing(8)
            .align_y(iced::Alignment::Center);
        if enabled || !a.skills.add.contains(&name) || a.skills.remove.contains(&name) {
            r = r.push(
                iced::widget::toggler(enabled)
                    .size(18)
                    .style(theme::switch_style)
                    .on_toggle(move |on| {
                        Message::Panel(PanelMsg::AgentSkill(id.clone(), name.clone(), Some(on)))
                    }),
            );
        }
        for b in buttons.into_iter().skip(1) {
            r = r.push(b);
        }
        Element::from(container(r).padding([7, 12]).width(Length::Fill))
    };
    let mut rows = Vec::new();
    for skill in a
        .active_skills
        .iter()
        .filter(|s| !a.skills.remove.contains(s))
    {
        let mut buttons = vec![pick(skill, "Turn off", Some(false))];
        if a.skills.add.contains(skill) {
            buttons.push(pick(skill, "Reset", None));
        }
        rows.push(row_of(skill, true, buttons));
    }
    for skill in &a.skills.remove {
        rows.push(row_of(
            skill,
            false,
            vec![
                pick(skill, "Turn on", Some(true)),
                pick(skill, "Reset", None),
            ],
        ));
    }
    for skill in a.skills.add.iter().filter(|s| !a.active_skills.contains(s)) {
        rows.push(row_of(skill, true, vec![pick(skill, "Reset", None)]));
    }
    if !rows.is_empty() {
        rows.push(
            text("Changes apply the next time the agent starts.")
                .size(11)
                .color(DIM)
                .into(),
        );
    }
    rows
}

pub(crate) fn agent_status(state: &str) -> Status {
    match state {
        "working" | "starting" => Status::Working,
        "blocked" => Status::Blocked,
        "done" => Status::Done,
        "idle" => Status::Idle,
        _ => Status::Unknown,
    }
}

/// The reason typed for a rejection, or nothing when it is blank.
fn reason_of(typed: &str) -> Option<String> {
    Some(typed.trim().to_owned()).filter(|r| !r.is_empty())
}

/// A task whose run can still be cancelled: it has started and has not ended.
fn tail_is_empty(o: &std::collections::HashMap<String, String>, pane: &str) -> bool {
    o.get(pane).is_none_or(|t| t.trim().is_empty())
}

fn msg_hot(status: button::Status) -> bool {
    matches!(status, button::Status::Hovered | button::Status::Pressed)
}

fn has_run(state: &str) -> bool {
    !matches!(
        state,
        "new" | "done" | "completed" | "archived" | "cancelled"
    )
}

pub(crate) fn task_state_color(state: &str) -> Color {
    match state {
        "done" | "completed" | "archived" => OK,
        "blocked" | "waiting" => WARN,
        "working" | "in_progress" | "running" => ACCENT,
        _ => DIM,
    }
}

/// One KPI cell (`.kpi`): pixel label, big value, optional muted subtitle.
pub(crate) fn kpi_cell<'a>(
    label: &'static str,
    value: String,
    color: Color,
    sub: Option<String>,
) -> Element<'a, Message> {
    kpi_inner(label, value, color, sub, None)
}

fn kpi_inner<'a>(
    label: &'static str,
    value: String,
    color: Color,
    sub: Option<String>,
    spark: Option<u32>,
) -> Element<'a, Message> {
    let mut body = column![
        theme::label(label),
        text(value).font(theme::MONO_BOLD).size(30).color(color),
    ]
    .spacing(4);
    if let Some(sub) = sub {
        body = body.push(text(sub).size(11).color(DIM));
    }
    let content: Element<'a, Message> = match spark {
        Some(frame) => row![
            body,
            container(text("")).width(Length::Fill),
            Canvas::new(Prism {
                preset: Preset::SPARK,
                frame,
            })
            .width(120)
            .height(26),
        ]
        .align_y(iced::Alignment::End)
        .into(),
        None => body.into(),
    };
    container(content)
        .width(Length::Fill)
        .padding([12, 16])
        .style(kpi_cell_style)
        .into()
}

/// `.sq`: 18 px square with the state glyph, filled with the state color.
pub(crate) fn agent_square<'a>(status: Status) -> Element<'a, Message> {
    let color = status_color(status);
    container(
        text(status_glyph(status))
            .font(theme::MONO_BOLD)
            .size(10)
            .color(theme::BG),
    )
    .center(18)
    .style(move |_| iced::widget::container::Style {
        background: Some(Background::Color(color)),
        border: iced::Border {
            radius: 3.0.into(),
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    })
    .into()
}

/// `.badge`: small solid pill.
pub(crate) fn badge<'a>(label: impl ToString, color: Color) -> Element<'a, Message> {
    container(
        text(label.to_string())
            .font(theme::MONO_BOLD)
            .size(10.5)
            .color(theme::BG),
    )
    .padding([2, 7])
    .style(theme::badge_style(color))
    .into()
}

/// `.btn`-family button body: 34 px high, label centered.
fn btn_body<'a>(label: impl ToString) -> Element<'a, Message> {
    container(text(label.to_string()).size(13))
        .center_y(Length::Fill)
        .into()
}

pub(crate) fn btn<'a>(label: impl ToString, msg: PanelMsg) -> Element<'a, Message> {
    button(btn_body(label))
        .padding([0, 12])
        .height(32)
        .style(theme::outline_button_style(false))
        .on_press(Message::Panel(msg))
        .into()
}

#[allow(dead_code)]
pub(crate) fn btn_ghost<'a>(label: impl ToString, msg: PanelMsg) -> Element<'a, Message> {
    button(btn_body(label))
        .padding([0, 12])
        .height(32)
        .style(theme::outline_button_style(true))
        .on_press(Message::Panel(msg))
        .into()
}

/// A quiet button (`Refresh`): no border, dim text.
pub(crate) fn btn_quiet<'a>(label: impl ToString, msg: PanelMsg) -> Element<'a, Message> {
    button(btn_body(label))
        .padding([0, 12])
        .height(32)
        .style(theme::outline_button_style(true))
        .on_press(Message::Panel(msg))
        .into()
}

pub(crate) fn btn_link<'a>(label: impl ToString, msg: PanelMsg) -> Element<'a, Message> {
    button(btn_body(label))
        .padding([0, 8])
        .height(32)
        .style(theme::link_button_style())
        .on_press(Message::Panel(msg))
        .into()
}

pub(crate) fn btn_primary<'a>(label: impl ToString, msg: PanelMsg) -> Element<'a, Message> {
    button(btn_body(label))
        .padding([0, 12])
        .height(32)
        .style(primary_button_style())
        .on_press(Message::Panel(msg))
        .into()
}

/// `.card`: optional pixel-font title with something on the right, a hairline, then the body.
pub(crate) fn card<'a>(
    title: &'static str,
    right: Option<Element<'a, Message>>,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut head = row![theme::label(title), container(text("")).width(Length::Fill)]
        .align_y(iced::Alignment::Center);
    if let Some(right) = right {
        head = head.push(right);
    }
    container(column![
        container(head).padding([10, 14]).width(Length::Fill),
        container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(|_| iced::widget::container::Style {
                background: Some(Background::Color(LINE)),
                ..iced::widget::container::Style::default()
            }),
        container(body).padding([12, 14]).width(Length::Fill),
    ])
    .width(Length::Fill)
    .style(card_style)
    .into()
}

/// One message of the communication feed (`.msg`): time, emitter (each with its own colour), a tag (topic or
/// kind, coloured by what it means) and the text. `with_position` adds the partition and offset (Events).
pub(crate) fn message_row<'a>(m: &BusMessage, with_position: bool) -> Element<'a, Message> {
    let time: String = m.ts.replace('T', " ").chars().skip(11).take(8).collect();
    let time = if time.is_empty() {
        "--:--:--".to_owned()
    } else {
        time
    };
    let (_, body) = feed_line(m);
    let tag = if m.topic.is_empty() {
        m.kind.clone()
    } else {
        format!("#{}", m.topic)
    };
    let tag_color = theme::kind_color(&m.kind, !m.topic.is_empty());
    let who_color = theme::emitter_color(&m.from);
    let mut who = row![
        container(text("").width(8).height(8)).style(move |_| iced::widget::container::Style {
            background: Some(Background::Color(who_color)),
            border: iced::Border {
                radius: 2.0.into(),
                ..iced::Border::default()
            },
            ..iced::widget::container::Style::default()
        }),
        text(m.from.clone()).size(13).color(who_color),
        container(text(tag).size(11).color(tag_color))
            .padding([0, 6])
            .style(move |_| iced::widget::container::Style {
                border: iced::Border {
                    color: tag_color,
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..iced::widget::container::Style::default()
            }),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if !m.topic.is_empty() && !m.kind.is_empty() && m.kind != "msg" {
        who = who.push(text(m.kind.clone()).size(11).color(tag_color));
    }
    if !m.to.is_empty() {
        who = who.push(text(format!("→ {}", m.to)).size(12).color(DIM));
    }
    if with_position {
        who = who.push(
            text(format!("p{} · #{}", m.partition, m.offset))
                .size(11)
                .color(DIM),
        );
    }
    column![
        container(
            row![
                text(time).size(13).color(DIM).width(80),
                column![who, text(body).size(13).color(theme::FG)].spacing(2),
            ]
            .spacing(12),
        )
        .padding([10, 0]),
        container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(|_| iced::widget::container::Style {
                background: Some(Background::Color(LINE)),
                ..iced::widget::container::Style::default()
            }),
    ]
    .into()
}

/// `.alert`: tinted box for a stalled run or an error.
pub(crate) fn alert<'a>(content: Element<'a, Message>, color: Color) -> Element<'a, Message> {
    container(content)
        .padding([10, 14])
        .width(Length::Fill)
        .style(theme::alert_style(color))
        .into()
}

impl Panel {
    pub fn update(&mut self, message: PanelMsg, ctx: &Ctx<'_>) -> Task<Message> {
        if let Some(summary) = failure_summary(&message, self.armed.as_ref()) {
            self.fail_summary = summary;
        }
        let task = self.update_inner(message, ctx);
        self.sync_toast();
        self.refresh_run_graph(ctx);
        task
    }

    /// A new notice (the panel's or the config tabs') becomes a toast and a line of the history.
    fn sync_toast(&mut self) {
        let current = if matches!(self.tab, Tab::Skills | Tab::Git | Tab::Events | Tab::Models)
            && !self.cfg.notice.is_empty()
        {
            self.cfg.notice.clone()
        } else {
            self.notice.clone()
        };
        if current != self.seen_notice {
            self.seen_notice = current.clone();
            if !current.is_empty() {
                self.history.push((
                    clock_now(),
                    current
                        .trim_start_matches(crate::ui::INFO)
                        .replace('\n', " "),
                ));
                let extra = self.history.len().saturating_sub(crate::ui::HISTORY);
                self.history.drain(..extra);
                self.toasts.push(crate::ui::Toast::new(current));
                // at most 3: a 4th pushes the oldest out
                let over = self.toasts.len().saturating_sub(3);
                self.toasts.drain(..over);
            }
        }
    }

    /// Asks the API for the tail of every agent pane (the cards of Agents show it as live output).
    fn load_outputs(&self, ctx: &Ctx<'_>) -> Task<Message> {
        let api = ctx.api.clone();
        let panes: Vec<String> = ctx
            .data
            .agents
            .iter()
            .filter(|a| !a.pane.is_empty())
            .map(|a| a.pane.clone())
            .collect();
        if panes.is_empty() {
            return Task::none();
        }
        Task::perform(
            async move {
                panes
                    .into_iter()
                    .filter_map(|p| actions::pane_tail(&api, &p, 4).ok().map(|t| (p, t)))
                    .collect::<Vec<_>>()
            },
            |l| Message::Panel(PanelMsg::Outputs(l)),
        )
    }

    /// The run shown in Runs: the picked one, else the first.
    fn shown_run<'a>(&self, ctx: &Ctx<'a>) -> Option<&'a shepherd_app_core::data::RunRow> {
        self.run
            .as_deref()
            .and_then(|id| ctx.data.runs.iter().find(|r| r.run.id == id))
            .or_else(|| ctx.data.runs.first())
    }

    /// Loads the graph of the shown run's pattern when it changed (it is a small file read).
    fn refresh_run_graph(&mut self, ctx: &Ctx<'_>) {
        let Some(run) = self.shown_run(ctx) else {
            self.run_graph = None;
            return;
        };
        if self.run_graph.as_ref().map(|(n, _)| n.as_str()) == Some(run.run.pattern.as_str()) {
            return;
        }
        self.run_graph = shepherd_app_core::pattern_graph::load(&ctx.data.root, &run.run.pattern)
            .ok()
            .map(|v| (run.run.pattern.clone(), v.graph));
    }

    /// The toast to draw now, if any (a success goes away after a few seconds).
    pub fn toasts(&self) -> Vec<(usize, &crate::ui::Toast)> {
        self.toasts
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.expired())
            .collect()
    }

    fn update_inner(&mut self, message: PanelMsg, ctx: &Ctx<'_>) -> Task<Message> {
        self.cfg.set_api(&ctx.api);
        // any action other than arming disarms
        if !matches!(message, PanelMsg::Arm(_) | PanelMsg::Confirm) {
            self.armed = None;
        }
        let api = ctx.api.clone();
        let root = ctx.data.root.display().to_string();
        match message {
            PanelMsg::Tab(tab) => {
                self.tab = tab;
                self.comm_open = false;
                self.filter.clear();
                self.chip = 0;
                if tab == Tab::Agents {
                    return self.load_outputs(ctx);
                }
                if let Some(task) = self.load_config_tab(tab, ctx) {
                    return task;
                }
                if tab == Tab::Monitor {
                    self.cfg.loading = true;
                    return crate::config_panel::ConfigPanel::load_topics(&ctx.api);
                }
                if tab == Tab::Patterns && self.edit.is_none() {
                    if let Some(first) = self
                        .pattern_sel
                        .clone()
                        .or_else(|| ctx.data.patterns.first().map(|p| p.name.clone()))
                    {
                        return self.update(PanelMsg::SelectPattern(first), ctx);
                    }
                }
            }
            PanelMsg::Cfg(msg) => return self.cfg.update(msg, &ctx.data.root),
            PanelMsg::Filter(t) => self.filter = t,
            PanelMsg::Chip(i) => self.chip = i,
            PanelMsg::DismissToast(i) => {
                if i < self.toasts.len() {
                    self.toasts.remove(i);
                }
            }
            PanelMsg::ActivityWindow(i) => self.activity_window = i,
            PanelMsg::OpenTopic(name) => {
                return Task::batch([
                    Task::done(Message::Panel(PanelMsg::Tab(Tab::Events))),
                    Task::done(Message::Panel(PanelMsg::Cfg(
                        crate::config_panel::CfgMsg::SelectTopic(name),
                    ))),
                ]);
            }
            PanelMsg::CommClose => {
                self.comm_open = false;
                self.tab = Tab::Runs;
            }
            PanelMsg::CommToggle(who) => {
                if !self.comm_hidden.remove(&who) {
                    self.comm_hidden.insert(who);
                }
            }
            PanelMsg::CommTopic(t) => self.comm_topic = t,
            PanelMsg::CommText(t) => self.comm_text = t,
            PanelMsg::CommSend => {
                let text = self.comm_text.trim().to_owned();
                let topic = self.comm_topic.clone().or_else(|| {
                    self.monitor
                        .as_deref()
                        .and_then(|id| ctx.data.runs.iter().find(|r| r.run.id == id))
                        .and_then(|r| r.run.topics.first().cloned())
                });
                let Some(topic) = topic else {
                    self.notice = crate::ui::invalid("This run has no topic to write to.");
                    return Task::none();
                };
                if text.is_empty() {
                    self.notice = crate::ui::invalid("Write something to send.");
                    return Task::none();
                }
                self.comm_text.clear();
                return Task::perform(
                    async move {
                        actions::topic_publish(&api, &topic, &text)
                            .map(|_| format!("Sent to #{topic}."))
                    },
                    |r| Message::Panel(PanelMsg::Done(r)),
                );
            }
            PanelMsg::SnapAgent(a) => self.snap_capture_agent = Some(a),
            PanelMsg::SnapFilterAgent(a) => self.snap_agent_filter = a,
            PanelMsg::SnapFilterReason(r) => self.snap_reason_filter = r,
            PanelMsg::NewPattern => {
                // the spec: a new pattern is a copy of the orchestrator-worker template, saved in this project
                if let Ok(view) = pattern_graph::load(&ctx.data.root, "orchestrator-worker") {
                    let mut editor = PatternEditor::from_view(view);
                    editor.copy_as("new-pattern");
                    self.tab = Tab::Patterns;
                    self.pattern_sel = None;
                    self.edit = Some(editor);
                    self.notice = "New pattern new-pattern from the orchestrator-worker template. Rename it, then save.".into();
                    return Task::none();
                }
                use shepherd_app_core::shepherd_core::patterns::{
                    Pattern, PatternAgent, PatternKind, PatternSource, Topic,
                };
                let pattern = Pattern {
                    name: "my-pattern".into(),
                    kind: PatternKind::Blackboard,
                    description: "Describe what this team does".into(),
                    topics: vec![Topic {
                        name: "work".into(),
                        partitions: 1,
                        key: None,
                        filter: None,
                    }],
                    agents: vec![PatternAgent {
                        id: "a1".into(),
                        role: "coder".into(),
                        publishes: vec!["work".into()],
                        consumes: vec!["work".into()],
                        ..PatternAgent::default()
                    }],
                };
                let graph = pattern_graph::layout(&pattern);
                self.tab = Tab::Patterns;
                self.pattern_sel = None;
                let mut editor = PatternEditor::from_view(PatternGraphView {
                    pattern,
                    source: PatternSource::Project,
                    path: None,
                    problems: vec![],
                    graph,
                });
                editor.mark_new();
                self.edit = Some(editor);
                self.notice = "New pattern: name it, add agents and topics, then Save.".into();
            }
            PanelMsg::Outputs(list) => self.outputs = list.into_iter().collect(),
            PanelMsg::SelectRun(id) => {
                self.tab = Tab::Runs;
                self.run = Some(id);
            }
            PanelMsg::SelectAgent(id) => {
                self.tab = Tab::Agents;
                self.agent = Some(id);
                self.monitor = None;
                return self.load_feed(ctx);
            }
            PanelMsg::SelectTask(id) => {
                self.tab = Tab::Tasks;
                self.task = Some(id);
                self.task_form = None;
            }
            PanelMsg::SelectPattern(name) => {
                self.pattern_sel = Some(name.clone());
                self.edit = None;
                return Task::perform(
                    async move { pattern_graph::load(std::path::Path::new(&root), &name) },
                    |r| Message::Panel(PanelMsg::PatternLoaded(r)),
                );
            }
            PanelMsg::PickAndLaunch(name) => {
                self.pattern = Some(Choice {
                    name: name.clone(),
                    label: name,
                });
                return self.update(PanelMsg::OpenLauncher(None), ctx);
            }
            PanelMsg::PatternLoaded(Ok(view)) => self.edit = Some(PatternEditor::from_view(view)),
            PanelMsg::EditPattern(msg) => {
                let saved = matches!(msg, EditMsg::Save);
                if let Some(edit) = self.edit.as_mut() {
                    if let Some(note) = edit.update(msg, &ctx.data.root) {
                        self.notice = note;
                    }
                    if saved {
                        self.pattern_sel = Some(edit.editor.pattern.name.clone());
                        return Task::done(Message::Reload);
                    }
                }
            }
            PanelMsg::PatternLoaded(Err(e)) => self.notice = e,
            PanelMsg::OpenLauncher(task) => {
                self.tab = Tab::Runs;
                self.launching = true;
                self.launch_task = task;
                let prompt = task
                    .and_then(|id| ctx.data.task(id))
                    .map(task_prompt)
                    .unwrap_or_default();
                self.objective = text_editor::Content::with_text(&prompt);
                if self.pattern.is_none() {
                    self.pattern = ctx.data.patterns.first().map(|p| Choice {
                        name: p.name.clone(),
                        label: p.name.clone(),
                    });
                }
            }
            PanelMsg::CloseLauncher => self.launching = false,
            PanelMsg::PickPattern(choice) => self.pattern = Some(choice),
            PanelMsg::Edit(action) => self.objective.perform(action),
            PanelMsg::Launch => {
                let first = ctx.data.patterns.first().map(|p| Choice {
                    name: p.name.clone(),
                    label: p.name.clone(),
                });
                let Some(pattern) = self.pattern.clone().or(first) else {
                    self.notice = crate::ui::invalid("Pick a pattern first.");
                    return Task::none();
                };
                let text = self.objective.text();
                let objective = match self.launch_task.and_then(|id| ctx.data.task(id)) {
                    Some(task) => Some(ensure_task_prefix(&text, task)),
                    None => Some(text.trim().to_owned()).filter(|t| !t.is_empty()),
                };
                let workspace = ctx.snapshot.focused_workspace_id.clone();
                self.notice = format!("Starting {}…", pattern.name);
                return Task::perform(
                    async move {
                        actions::start_pattern_run(
                            &api,
                            &pattern.name,
                            objective.as_deref(),
                            Some(&root),
                            workspace.as_deref(),
                        )
                        .map(|launched| {
                            let agents = launched.run["agents"].as_array().map_or(0, Vec::len);
                            format!(
                                "Run {} started with {agents} agent(s){}",
                                launched.run["id"].as_str().unwrap_or("?"),
                                if launched.over_limit {
                                    " (over the concurrent-agent limit)"
                                } else {
                                    ""
                                }
                            )
                        })
                    },
                    |r| Message::Panel(PanelMsg::Done(r)),
                );
            }
            PanelMsg::Control(id, control) => {
                if control == RunControl::Stop && self.armed != Some(Armed::StopRun(id.clone())) {
                    self.armed = Some(Armed::StopRun(id));
                    return Task::none();
                }
                self.armed = None;
                return Task::perform(
                    async move {
                        actions::control_run(&api, &id, control, Some(&root)).map(|_| match control
                        {
                            RunControl::Pause => format!(
                                "Paused {id}. Its agents finish their current step, then wait."
                            ),
                            RunControl::Resume => format!("Resumed {id}."),
                            RunControl::Stop => format!("Stopped {id} and closed its agents."),
                        })
                    },
                    |r| Message::Panel(PanelMsg::Done(r)),
                );
            }
            PanelMsg::Arm(what) => {
                self.armed = Some(what);
                self.armed_at = Some(std::time::Instant::now());
            }
            PanelMsg::Confirm => {
                let Some(armed) = self.armed.take() else {
                    return Task::none();
                };
                match armed {
                    Armed::StopRun(id) => {
                        return self.update(PanelMsg::Control(id, RunControl::Stop), ctx)
                    }
                    Armed::DeletePattern(name) => {
                        self.edit = None;
                        self.pattern_sel = None;
                        return Task::perform(
                            async move {
                                actions::delete_pattern(&api, &name, &root)
                                    .map(|_| format!("Deleted pattern {name}."))
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                    Armed::DeleteTask(id) => {
                        if self.task == Some(id) {
                            self.task = None;
                        }
                        return Task::perform(
                            async move {
                                actions::delete_task(&api, id, &root)
                                    .map(|_| format!("Deleted task #{id}."))
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                    Armed::CancelRun(id) => {
                        return Task::perform(
                            async move {
                                actions::cancel_run(&api, id, &root).map(|agents| {
                                    format!("Cancelled the run of task #{id} and closed {agents} agent(s)")
                                })
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                    Armed::DeleteRun(id) => {
                        if self.run.as_deref() == Some(id.as_str()) {
                            self.run = None;
                        }
                        return Task::perform(
                            async move {
                                actions::delete_run(&api, &id, Some(&root), true).map(|agents| {
                                    format!("Deleted run {id} and cleaned {agents} agent(s)")
                                })
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                    Armed::CleanRuns => {
                        return Task::perform(
                            async move {
                                actions::clean_runs(&api, Some(&root)).map(|(runs, agents)| {
                                    format!("Cleaned {runs} run(s) and {agents} agent(s)")
                                })
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                    Armed::CleanBus | Armed::CleanBusForce => {
                        let force = armed == Armed::CleanBusForce;
                        return Task::perform(
                            async move {
                                actions::clean_bus(&api, force)
                                    .map(|n| format!("Cleared {n} message(s)"))
                            },
                            |r| match r {
                                Err(e) if e.contains("confirmation_required") => {
                                    Message::Panel(PanelMsg::Arm(Armed::CleanBusForce))
                                }
                                other => Message::Panel(PanelMsg::Done(other)),
                            },
                        );
                    }
                    Armed::RestoreSnapshot(id) => {
                        self.snap_detail = None;
                        self.snap_compare = None;
                        let root = snapshots::restore_root(ctx.data, &id);
                        return Task::perform(
                            async move {
                                actions::snapshot_restore(&api, &id, &root).map(|r| {
                                    format!(
                                        "Restored {} ({} file(s)); the state before is snapshot {}",
                                        r.restored.id,
                                        r.restored_files.len(),
                                        r.pre_restore.id
                                    )
                                })
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                    Armed::DeleteSnapshot(id) => {
                        if self.snap.as_deref() == Some(id.as_str()) {
                            self.snap = None;
                        }
                        self.snap_detail = None;
                        self.snap_compare = None;
                        return Task::perform(
                            async move {
                                actions::snapshot_delete(&api, &id, &root)
                                    .map(|_| format!("Deleted snapshot {id}."))
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                    Armed::PruneSnapshots(agent) => {
                        self.snap_detail = None;
                        self.snap_compare = None;
                        let root = snapshots::restore_root(
                            ctx.data,
                            &self.snap_id(ctx).unwrap_or_default(),
                        );
                        return Task::perform(
                            async move {
                                actions::snapshot_prune(&api, &agent, &root).map(|r| {
                                    format!(
                                        "Pruned {} old snapshot(s) of {agent}; kept {}",
                                        r.pruned.len(),
                                        r.kept
                                    )
                                })
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                    Armed::ApproveGate(id, gate) => {
                        return Task::perform(
                            async move {
                                actions::approve_gate(&api, id, &gate.target(), &root)
                                    .map(|_| match &gate {
                                        Gate::Plan => format!("Approved the plan of task #{id}."),
                                        Gate::PushPr(n) => format!(
                                            "Approved subtask {n}. Pushed its branch and opened its pull request."
                                        ),
                                    })
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                    Armed::RejectGate(id, gate) => {
                        let reason = reason_of(&std::mem::take(&mut self.reject_reason));
                        return Task::perform(
                            async move {
                                actions::reject_gate(
                                    &api,
                                    id,
                                    &gate.target(),
                                    reason.as_deref(),
                                    &root,
                                )
                                .map(|_| {
                                    let why = reason
                                        .as_deref()
                                        .map(|r| format!(" ({r})"))
                                        .unwrap_or_default();
                                    match &gate {
                                        Gate::Plan => {
                                            format!("Rejected the plan of task #{id}{why}. Nothing was started.")
                                        }
                                        Gate::PushPr(n) => {
                                            format!("Rejected subtask {n}{why}. Nothing was pushed.")
                                        }
                                    }
                                })
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                    Armed::ArchiveDone => {
                        let done: Vec<u32> = ctx
                            .data
                            .tasks
                            .iter()
                            .filter(|t| {
                                matches!(
                                    t.status.state.to_lowercase().as_str(),
                                    "done" | "completed"
                                )
                            })
                            .map(|t| t.status.id)
                            .collect();
                        return Task::perform(
                            async move {
                                let archived = done
                                    .iter()
                                    .filter(|id| {
                                        actions::archive_task(&api, **id, Some(&root)).is_ok()
                                    })
                                    .count();
                                Ok(format!(
                                    "Archived {archived} done task{}.",
                                    if archived == 1 { "" } else { "s" }
                                ))
                            },
                            |r| Message::Panel(PanelMsg::Done(r)),
                        );
                    }
                }
            }
            PanelMsg::Done(Ok(note)) => {
                self.notice = note;
                self.launching = false;
                self.task_form = None;
                return Task::done(Message::Reload);
            }
            PanelMsg::Notify(text) => self.notice = text,
            PanelMsg::Done(Err(e)) => self.notice = refusal(self.fail_summary, &e),
            PanelMsg::SnapSelect(id) => {
                self.snap = Some(id);
                self.snap_detail = None;
                self.snap_compare = None;
                return self.snap_show(ctx);
            }
            PanelMsg::SnapShown(id, shown) => {
                if self.snap_id(ctx).as_deref() == Some(id.as_str()) {
                    self.snap_detail = Some((id, shown));
                }
            }
            PanelMsg::SnapCapture(agent) => return self.snap_capture(agent, ctx),
            PanelMsg::AgentSkill(agent, skill, enable) => {
                let (api, root) = (ctx.api.clone(), ctx.data.root.display().to_string());
                return Task::perform(
                    async move {
                        actions::agent_skill(&api, &agent, &skill, enable, &root).map(|_| {
                            format!(
                                "Saved: {skill} {} for {agent}. It applies the next time the agent starts.",
                                match enable {
                                    Some(true) => "on",
                                    Some(false) => "off",
                                    None => "reset",
                                }
                            )
                        })
                    },
                    |r| Message::Panel(PanelMsg::Done(r)),
                );
            }
            PanelMsg::SnapCompare(id) => {
                self.snap_compare = None;
                let api = ctx.api.clone();
                let root = snapshots::restore_root(ctx.data, &id);
                return Task::perform(
                    async move {
                        let diff = actions::snapshot_compare(&api, &id, &root).map(Box::new);
                        (id, diff)
                    },
                    |(id, diff)| Message::Panel(PanelMsg::SnapCompared(id, diff)),
                );
            }
            PanelMsg::SnapCompared(id, diff) => {
                if diff.is_ok() {
                    self.notice =
                        crate::ui::info(&format!("Compared {id} with the worktree as it is now."));
                }
                if self.snap_id(ctx).as_deref() == Some(id.as_str()) {
                    self.snap_compare = Some((id, diff));
                }
            }
            PanelMsg::ShowMonitor(run) => {
                self.tab = Tab::Runs;
                self.comm_open = true;
                self.comm_topic = None;
                self.comm_hidden.clear();
                self.monitor = (!run.is_empty()).then_some(run);
                return self.load_feed(ctx);
            }
            PanelMsg::Feed(feed) => self.feed = feed,
            // Spawn agent form
            PanelMsg::OpenSpawnAgent => {
                self.spawning_agent = true;
                self.spawn_role = None;
                self.spawn_workspace = None;
                self.spawn_vendor = None;
                self.spawn_model.clear();
                self.spawn_task_text.clear();
                self.spawn_task_id = None;
                self.spawn_worktree = false;
                // Pre-select first role and workspace
                if let Some(first_role) = ctx.data.models.roles.iter().next() {
                    self.spawn_role = Some(Choice {
                        name: first_role.0.clone(),
                        label: first_role.0.clone(),
                    });
                }
                if let Some(first_ws) = ctx.snapshot.workspaces.first() {
                    self.spawn_workspace = Some(Choice {
                        name: first_ws.workspace_id.clone(),
                        label: first_ws.label.clone(),
                    });
                }
            }
            PanelMsg::CloseSpawnAgent => self.spawning_agent = false,
            PanelMsg::SpawnAgentRole(choice) => {
                self.spawn_role = Some(choice.clone());
                // Auto-select vendor from role
                if let Some(role) = ctx.data.models.roles.get(&choice.name) {
                    if !role.vendor.is_empty() {
                        self.spawn_vendor = Some(Choice {
                            name: role.vendor.clone(),
                            label: role.vendor.clone(),
                        });
                    }
                }
            }
            PanelMsg::SpawnAgentWorkspace(choice) => self.spawn_workspace = Some(choice),
            PanelMsg::SpawnAgentVendor(choice) => self.spawn_vendor = Some(choice),
            PanelMsg::SpawnAgentModel(model) => self.spawn_model = model,
            PanelMsg::SpawnAgentTask(text) => self.spawn_task_text = text,
            PanelMsg::SpawnAgentWorktree(on) => self.spawn_worktree = on,
            PanelMsg::SpawnAgent => {
                let Some(role) = self.spawn_role.clone() else {
                    self.notice = "Pick a role first.".into();
                    return Task::none();
                };
                let task_text = self.spawn_task_text.trim().to_owned();
                if task_text.is_empty() {
                    self.notice = "Enter a task description.".into();
                    return Task::none();
                }
                let workspace = self.spawn_workspace.as_ref().map(|c| c.name.clone());
                let vendor = self.spawn_vendor.as_ref().map(|c| c.name.clone());
                let model = if self.spawn_model.is_empty() {
                    None
                } else {
                    Some(self.spawn_model.clone())
                };
                let root = ctx.data.root.display().to_string();
                let worktree = self.spawn_worktree;
                let task_id = self.spawn_task_id;
                self.notice = format!("Starting {} agent…", role.name);
                return Task::perform(
                    async move {
                        actions::spawn_agent(
                            &api,
                            &role.name,
                            &task_text,
                            Some(&root),
                            workspace.as_deref(),
                            vendor.as_deref(),
                            model.as_deref(),
                            worktree,
                            task_id,
                        )
                        .map(|agent| {
                            format!(
                                "Launched {} in pane {} of {}.",
                                agent["id"].as_str().unwrap_or("?"),
                                agent["pane"].as_str().unwrap_or("?"),
                                agent["workspace"].as_str().unwrap_or("?")
                            )
                        })
                    },
                    |r| Message::Panel(PanelMsg::Done(r)),
                );
            }
            // Task form
            PanelMsg::TaskNew => {
                self.launching = false;
                self.importing = None;
                self.task_form = Some(TaskForm::new());
            }
            PanelMsg::RejectReason(text) => self.reject_reason = text,
            PanelMsg::ImportOpen => {
                self.launching = false;
                self.task_form = None;
                self.importing = Some(String::new());
            }
            PanelMsg::ImportInput(text) => {
                if self.importing.is_some() {
                    self.importing = Some(text);
                }
            }
            PanelMsg::ImportCancel => self.importing = None,
            PanelMsg::ImportSubmit => {
                let issue = self.importing.clone().unwrap_or_default();
                if issue.trim().is_empty() {
                    self.notice = "Type an issue number or URL".into();
                    return Task::none();
                }
                self.importing = None;
                return Task::perform(
                    async move {
                        actions::import_issue(&api, &issue, &root).map(|id| {
                            format!(
                                "Imported issue {} as task #{id}.",
                                issue.trim().trim_start_matches('#')
                            )
                        })
                    },
                    |r| Message::Panel(PanelMsg::Done(r)),
                );
            }
            PanelMsg::TaskEditOpen(id) => {
                if let Some(task) = ctx.data.task(id) {
                    self.launching = false;
                    self.task_form = Some(TaskForm::of(task));
                }
            }
            PanelMsg::TaskTitle(title) => {
                if let Some(form) = &mut self.task_form {
                    form.title = title;
                }
            }
            PanelMsg::TaskDescription(action) => {
                if let Some(form) = &mut self.task_form {
                    form.description.perform(action);
                }
            }
            PanelMsg::TaskCriteria(action) => {
                if let Some(form) = &mut self.task_form {
                    form.criteria.perform(action);
                }
            }
            PanelMsg::TaskSpec(action) => {
                if let Some(form) = &mut self.task_form {
                    form.spec.perform(action);
                }
            }
            PanelMsg::TaskCancel => self.task_form = None,
            PanelMsg::TaskSave => {
                let Some(form) = &self.task_form else {
                    return Task::none();
                };
                let title = form.title.trim().to_owned();
                if title.is_empty() {
                    self.notice = crate::ui::invalid("A task needs a title.");
                    return Task::none();
                }
                let description = form.description.text().trim().to_owned();
                let criteria = shepherd_app_core::data::parse_criteria(&form.criteria.text());
                let spec = form.spec.text();
                let editing = form.editing;
                let root = ctx.data.root.display().to_string();
                return Task::perform(
                    async move {
                        match editing {
                            None => {
                                actions::create_task(&api, &root, &title, &description, &criteria)
                                    .map(|id| format!("Created task #{id}."))
                            }
                            Some(id) => actions::update_task(
                                &api,
                                &root,
                                id,
                                &title,
                                &description,
                                Some(&spec),
                                &criteria,
                            )
                            .map(|_| format!("Saved task #{id}")),
                        }
                    },
                    |r| Message::Panel(PanelMsg::Done(r)),
                );
            }
            PanelMsg::RunTaskWithAgent(id) => {
                let Some(task) = ctx.data.task(id) else {
                    return Task::none();
                };
                let prompt = task_prompt(task);
                self.tab = Tab::Agents;
                let opened = self.update(PanelMsg::OpenSpawnAgent, ctx);
                self.spawn_task_text = prompt;
                self.spawn_task_id = Some(id);
                return opened;
            }
            // Agent actions
            PanelMsg::KillAgent(id) => {
                return Task::perform(
                    async move {
                        actions::kill_and_clean_agent(&api, &id)
                            .map(|_| format!("Closed {id}. Its worktree and snapshots stay."))
                    },
                    |r| Message::Panel(PanelMsg::Done(r)),
                );
            }
            PanelMsg::ResumeAgents => {
                let idle: Vec<String> = ctx
                    .data
                    .agents
                    .iter()
                    .filter(|a| a.state == "idle")
                    .map(|a| a.id.clone())
                    .collect();
                if idle.is_empty() {
                    self.notice = crate::ui::info("No idle agents to resume.");
                    return Task::none();
                }
                return Task::perform(
                    async move {
                        actions::resume_agents(&api).map(|_| {
                            format!(
                                "Resumed {} idle agent{}: {}.",
                                idle.len(),
                                if idle.len() == 1 { "" } else { "s" },
                                idle.join(", ")
                            )
                        })
                    },
                    |r| Message::Panel(PanelMsg::Done(r)),
                );
            }
            PanelMsg::AssignAgentTask(agent_id, task_id) => {
                let root = ctx.data.root.display().to_string();
                return Task::perform(
                    async move {
                        actions::assign_agent_task(&api, &agent_id, task_id, &root).map(|_| {
                            match task_id {
                                Some(t) => format!("Moved {agent_id} to task #{t}."),
                                None => format!("Moved {agent_id} to no task."),
                            }
                        })
                    },
                    |r| Message::Panel(PanelMsg::Done(r)),
                );
            }
        }
        Task::none()
    }

    /// After the data (re)loads: the Patterns tab opens on its first pattern when none was picked yet.
    /// The tabs that read slow things (git, the bus, skills) load when opened, off the UI thread.
    pub fn load_config_tab(&mut self, tab: Tab, ctx: &Ctx<'_>) -> Option<Task<Message>> {
        self.cfg.set_api(&ctx.api);
        use crate::config_panel::ConfigPanel as C;
        let root = &ctx.data.root;
        match tab {
            Tab::Skills => {
                self.cfg.loading = true;
                Some(C::load_skills(root))
            }
            Tab::Git => {
                self.cfg.loading = true;
                Some(C::load_git(root))
            }
            Tab::Events => {
                self.cfg.loading = true;
                Some(C::load_topics(&ctx.api))
            }
            Tab::Models => {
                self.cfg.loading = true;
                Some(C::load_models(root))
            }
            Tab::Snapshots => Some(self.snap_show(ctx)),
            _ => None,
        }
    }

    pub fn on_data(&mut self, ctx: &Ctx<'_>) -> Task<Message> {
        self.cfg.set_api(&ctx.api);
        self.refresh_run_graph(ctx);
        if let Some(id) = std::env::var("SHEPHERD_NATIVE_TASKFORM")
            .ok()
            .and_then(|v| v.strip_prefix("edit:").and_then(|n| n.parse::<u32>().ok()))
        {
            if self.task_form.is_none() && ctx.data.task(id).is_some() {
                let _ = self.update(PanelMsg::TaskEditOpen(id), ctx);
            }
        }
        if !std::mem::replace(&mut self.cfg_started, true) {
            let counts = crate::config_panel::ConfigPanel::load_counts(&ctx.data.root, &ctx.api);
            return match self.load_config_tab(self.tab, ctx) {
                Some(task) => Task::batch([task, counts]),
                None => counts,
            };
        }
        if self.tab == Tab::Snapshots {
            return self.snap_show(ctx);
        }
        if self.tab == Tab::Agents {
            return self.load_outputs(ctx);
        }
        if self.tab == Tab::Patterns && self.pattern_sel.is_none() {
            if let Some(first) = ctx.data.patterns.first().map(|p| p.name.clone()) {
                return self.update(PanelMsg::SelectPattern(first), ctx);
            }
        }
        Task::none()
    }

    /// Reloads the communication of the monitored run (called when files change, too).
    pub fn load_feed(&self, ctx: &Ctx<'_>) -> Task<Message> {
        let agents: Vec<String> = match self
            .monitor
            .as_deref()
            .and_then(|id| ctx.data.runs.iter().find(|r| r.run.id == id))
        {
            Some(row) => row.run.agents.iter().map(|a| a.id.clone()).collect(),
            None => self
                .agent
                .as_deref()
                .map(|id| vec![id.to_owned()])
                .unwrap_or_else(|| ctx.data.agents.iter().map(|a| a.id.clone()).collect()),
        };
        Task::perform(async move { bus_feed(&agents, 300) }, |feed| {
            Message::Panel(PanelMsg::Feed(feed))
        })
    }

    fn arm_button<'a>(
        &self,
        key: Armed,
        label: impl ToString,
        armed_label: impl ToString,
    ) -> Element<'a, Message> {
        let armed = self.armed.as_ref() == Some(&key);
        let approving = matches!(key, Armed::ApproveGate(..));
        let msg = if armed {
            PanelMsg::Confirm
        } else {
            PanelMsg::Arm(key)
        };
        let btn_label = if armed {
            armed_label.to_string()
        } else {
            label.to_string()
        };
        let content = btn_body(btn_label);
        if armed {
            // the bar along the bottom edge drains over `ARM_SECS`, then the button disarms
            let left = self.armed_at.map_or(0.0, |t| {
                (1.0 - t.elapsed().as_secs_f32() / ARM_SECS).clamp(0.0, 1.0)
            });
            let portion = (left * 1000.0) as u16;
            let bar = row![
                container(text(""))
                    .width(Length::FillPortion(portion.max(1)))
                    .height(3)
                    .style(|_| container::Style {
                        background: Some(Background::Color(theme::BG)),
                        ..container::Style::default()
                    }),
                container(text(""))
                    .width(Length::FillPortion(1000 - portion.min(999)))
                    .height(3),
            ];
            return iced::widget::stack![
                button(content)
                    .padding([0, 12])
                    .height(32)
                    .style(danger_button_style())
                    .on_press(Message::Panel(msg)),
                container(bar).align_bottom(Length::Fill).padding([0, 1]),
            ]
            .into();
        } else if approving {
            button(content)
                .padding([0, 12])
                .height(32)
                .style(primary_button_style())
        } else {
            button(content)
                .padding([0, 12])
                .height(32)
                .style(theme::destructive_button_style())
        }
        .on_press(Message::Panel(msg))
        .into()
    }

    /// The navigation of the sidebar (`ORCHESTRATE` and `INSPECT`): a square bullet, the name, a count (orange
    /// when something in that view needs the user) and the shortcut.
    pub fn nav<'a>(&self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let runs_warn = !ctx.data.gates.is_empty()
            || ctx.data.runs.iter().any(|r| {
                matches!(
                    r.run.state,
                    PatternRunState::Stalled | PatternRunState::Degraded | PatternRunState::Failed
                )
            });
        let agents_warn = ctx
            .data
            .agents
            .iter()
            .any(|a| a.state == "blocked" || !a.notice.is_empty());
        let counts = self.cfg.counts();
        let item = |tab: Tab, name: &'static str, key: &'static str| {
            let (count, warn) = match tab {
                Tab::Runs => (Some(ctx.data.runs.len()), runs_warn),
                Tab::Agents => (Some(ctx.data.agents.len()), agents_warn),
                Tab::Tasks => (
                    Some(
                        ctx.data
                            .tasks
                            .iter()
                            .filter(|t| {
                                !matches!(
                                    t.status.state.as_str(),
                                    "done" | "completed" | "archived" | "cancelled"
                                )
                            })
                            .count(),
                    ),
                    false,
                ),
                Tab::Patterns => (Some(ctx.data.patterns.len()), false),
                Tab::Skills => (Some(counts.0), false),
                Tab::Git => (self.cfg.git_count(), false),
                Tab::Events => (Some(counts.1), false),
                Tab::Models => (Some(counts.2), false),
                Tab::Snapshots => (Some(ctx.data.snapshots.len()), false),
                _ => (None, false),
            };
            let selected = self.tab == tab;
            let mut r = row![
                container(text(""))
                    .width(6)
                    .height(6)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(if selected {
                            ACCENT
                        } else {
                            theme::LINE2
                        })),
                        border: iced::Border {
                            radius: 1.0.into(),
                            ..iced::Border::default()
                        },
                        ..container::Style::default()
                    }),
                text(name)
                    .size(12.5)
                    .color(if selected { theme::HI } else { FG }),
                container(text("")).width(Length::Fill),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center);
            if let Some(count) = count {
                r = r.push(
                    container(text(count.to_string()).size(10.5).color(if warn {
                        theme::BG
                    } else {
                        DIM
                    }))
                    .padding([0, 6])
                    .style(move |_| {
                        let mut s = theme::count_style(false, false);
                        if warn {
                            s.background = Some(Background::Color(WARN));
                        }
                        s
                    }),
                );
            }
            if !key.is_empty() {
                r = r.push(
                    text(key)
                        .size(10)
                        .color(theme::FAINT)
                        .width(Length::Fixed(44.0)),
                );
            }
            button(container(r).center_y(Length::Fill))
                .width(Length::Fill)
                .height(30)
                .padding([0, 10])
                .style(item_style(selected))
                .on_press(Message::Panel(PanelMsg::Tab(tab)))
        };
        column![
            container(theme::label("ORCHESTRATE")).padding(pad(10.0, 8.0, 4.0, 8.0)),
            item(Tab::Overview, "Overview", "Ctrl+1"),
            item(Tab::Runs, "Runs", "Ctrl+2"),
            item(Tab::Agents, "Agents", "Ctrl+3"),
            item(Tab::Tasks, "Tasks", "Ctrl+4"),
            item(Tab::Patterns, "Patterns", "Ctrl+5"),
            container(theme::label("INSPECT")).padding(pad(14.0, 8.0, 4.0, 8.0)),
            item(Tab::Monitor, "Activity", "Ctrl+6"),
            item(Tab::Skills, "Skills", "Ctrl+7"),
            item(Tab::Git, "Git", "Ctrl+8"),
            item(Tab::Events, "Events", "Ctrl+9"),
            item(Tab::Models, "Models", ""),
            item(Tab::Snapshots, "Snapshots", ""),
        ]
        .spacing(2)
        .into()
    }

    pub fn view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        crate::ui::nav_reset();
        let body = match self.tab {
            Tab::Overview => self.overview_view(ctx),
            Tab::Runs => self.runs_view(ctx),
            Tab::Agents => self.agents_view(ctx),
            Tab::Tasks => self.tasks_view(ctx),
            Tab::Patterns => self.patterns_view(ctx),
            Tab::Monitor => self.monitor_view(ctx),
            Tab::Skills => self
                .cfg
                .skills_view(&self.filter, self.chip, &ctx.data.agents),
            Tab::Git => self.cfg.git_view(),
            Tab::Events => self.cfg.events_view(ctx.frame, &self.filter),
            Tab::Models => self.cfg.models_view(),
            Tab::Snapshots => self.snapshots_view(ctx),
        };
        if self.comm_open {
            return self.comm_view(ctx);
        }
        column![self.header(ctx), body].into()
    }

    /// The view's header (`title`, one line saying what is in it, and its actions at the top right).
    fn header<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let d = ctx.data;
        let waiting = d.gates.len();
        let (title, sub, actions): (&'static str, String, Vec<Element<'a, Message>>) = match self
            .tab
        {
            Tab::Overview => (
                "Overview",
                format!("Workspace in {}", d.root.display()),
                vec![
                    btn("+ New agent", PanelMsg::OpenSpawnAgent),
                    btn("+ New task", PanelMsg::TaskNew),
                    btn_primary("Run a pattern", PanelMsg::OpenLauncher(None)),
                ],
            ),
            Tab::Runs => {
                let running = d
                    .runs
                    .iter()
                    .filter(|r| matches!(r.run.state, PatternRunState::Running))
                    .count();
                let mut s = format!(
                    "{} runs: {running} running, {} finished.",
                    d.runs.len(),
                    d.runs
                        .iter()
                        .filter(|r| matches!(r.run.state, PatternRunState::Done))
                        .count()
                );
                if waiting > 0 {
                    s.push_str(&format!(" {waiting} gate waits for you."));
                }
                (
                    "Runs",
                    s,
                    vec![
                        self.arm_button(
                            Armed::CleanRuns,
                            "Clean finished",
                            "Confirm: delete finished",
                        ),
                        btn_primary("Run a pattern", PanelMsg::OpenLauncher(None)),
                    ],
                )
            }
            Tab::Agents => {
                let working = d.agents.iter().filter(|a| a.state == "working").count();
                let idle = d.agents.iter().filter(|a| a.state == "idle").count();
                let blocked = d.agents.iter().filter(|a| a.state == "blocked").count();
                (
                    "Agents",
                    format!(
                        "{} agents: {working} working, {idle} idle, {blocked} blocked",
                        d.agents.len()
                    ),
                    vec![
                        self.arm_button(
                            Armed::CleanRuns,
                            "Clean finished",
                            "Confirm: delete finished",
                        ),
                        btn("Resume idle", PanelMsg::ResumeAgents),
                        btn_primary("+ New agent", PanelMsg::OpenSpawnAgent),
                    ],
                )
            }
            Tab::Tasks => {
                let open = d
                    .tasks
                    .iter()
                    .filter(|t| {
                        !matches!(
                            t.status.state.as_str(),
                            "done" | "completed" | "archived" | "cancelled"
                        )
                    })
                    .count();
                let done = d
                    .tasks
                    .iter()
                    .filter(|t| {
                        matches!(t.status.state.as_str(), "done" | "completed" | "archived")
                    })
                    .count();
                let waiting = d
                    .tasks
                    .iter()
                    .flat_map(|t| t.status.subtasks.iter())
                    .filter(|s| s.state == "awaiting-approval")
                    .count();
                let mut s = format!("{} tasks: {open} open, {done} done.", d.tasks.len());
                if waiting > 0 {
                    s.push_str(&format!(" {waiting} subtask waits for your approval."));
                }
                (
                    "Tasks",
                    s,
                    vec![
                        text_input("Filter by number or title", &self.filter)
                            .on_input(|t| Message::Panel(PanelMsg::Filter(t)))
                            .id(crate::ui::FILTER_ID)
                            .style(theme::input_style)
                            .padding([8, 10])
                            .size(12)
                            .width(240)
                            .into(),
                        btn("Import issue", PanelMsg::ImportOpen),
                        btn_primary("+ New task", PanelMsg::TaskNew),
                    ],
                )
            }
            Tab::Patterns => (
                "Patterns",
                "Teams of agents and the topics they talk on. Stored as YAML.".to_owned(),
                vec![btn_primary("+ New pattern", PanelMsg::NewPattern)],
            ),
            Tab::Monitor => {
                let chip = |label: &'static str, i: usize| {
                    button(text(label).size(11.5))
                        .padding([4, 12])
                        .style(theme::chip_style(self.activity_window == i))
                        .on_press(Message::Panel(PanelMsg::ActivityWindow(i)))
                        .into()
                };
                (
                    "Activity",
                    "What every agent did over time, and how far each reader is behind on the bus"
                        .to_owned(),
                    vec![
                        chip("5 min", 0),
                        chip("30 min", 1),
                        chip("2 h", 2),
                        text("● live").size(11.5).color(ACCENT).into(),
                    ],
                )
            }
            Tab::Skills => (
                "Skills",
                "Agents load a skill when its description fits the work.".to_owned(),
                vec![
                    btn_quiet(
                        "Refresh",
                        PanelMsg::Cfg(crate::config_panel::CfgMsg::Refresh),
                    ),
                    btn_primary(
                        "+ New skill",
                        PanelMsg::Cfg(crate::config_panel::CfgMsg::SkillNewOpen),
                    ),
                ],
            ),
            Tab::Git => (
                "Git",
                format!("Worktrees of {}", d.root.display()),
                vec![
                    btn_quiet(
                        "Refresh",
                        PanelMsg::Cfg(crate::config_panel::CfgMsg::Refresh),
                    ),
                    btn(
                        "+ Branch",
                        PanelMsg::Cfg(crate::config_panel::CfgMsg::GitFormOpen(false)),
                    ),
                    btn_primary(
                        "+ Worktree",
                        PanelMsg::Cfg(crate::config_panel::CfgMsg::GitFormOpen(true)),
                    ),
                ],
            ),
            Tab::Events => (
                "Events",
                "The bus: topics, partitions and the messages on them".to_owned(),
                vec![
                    btn_quiet(
                        "Refresh",
                        PanelMsg::Cfg(crate::config_panel::CfgMsg::Refresh),
                    ),
                    btn(
                        "Send message",
                        PanelMsg::Cfg(crate::config_panel::CfgMsg::TopicFormOpen(true)),
                    ),
                    btn_primary(
                        "+ New topic",
                        PanelMsg::Cfg(crate::config_panel::CfgMsg::TopicFormOpen(false)),
                    ),
                ],
            ),
            Tab::Models => (
                "Models",
                "Which command runs each vendor, and which vendor and model each role uses."
                    .to_owned(),
                vec![
                    btn(
                        "+ Vendor",
                        PanelMsg::Cfg(crate::config_panel::CfgMsg::NewVendor),
                    ),
                    btn(
                        "+ Role",
                        PanelMsg::Cfg(crate::config_panel::CfgMsg::NewRole),
                    ),
                ],
            ),
            Tab::Snapshots => {
                let agents: Vec<String> = d.agents.iter().map(|a| a.id.clone()).collect();
                let current = self.capture_agent(ctx);
                let mut actions: Vec<Element<'a, Message>> =
                    vec![text("Capture").size(12).color(DIM).into()];
                actions.push(
                    pick_list(agents, current.clone(), |a| {
                        Message::Panel(PanelMsg::SnapAgent(a))
                    })
                    .text_size(12)
                    .padding([8, 10])
                    .into(),
                );
                if let Some(agent) = current {
                    actions.push(btn_primary("Capture now", PanelMsg::SnapCapture(agent)));
                }
                (
                    "Snapshots",
                    format!(
                        "{} saved copies of agent worktrees, newest first. Restore one to undo an agent's changes.",
                        d.snapshots.len()
                    ),
                    actions,
                )
            }
        };
        crate::ui::page_header(title, sub, actions)
    }

    /// The window now looks at another session: forget what was picked and loaded from the previous one.
    pub fn session_changed(&mut self) {
        self.run = None;
        self.agent = None;
        self.task = None;
        self.pattern_sel = None;
        self.edit = None;
        self.launching = false;
        self.launch_task = None;
        self.armed = None;
        self.monitor = None;
        self.feed.clear();
        self.spawning_agent = false;
        self.notice.clear();
        self.cfg = crate::config_panel::ConfigPanel::default();
        self.cfg_started = false;
    }

    /// Events stay live: when the state files change, the topics and the open topic's messages are read again.
    pub fn refresh_events(&self) -> Task<Message> {
        self.cfg.reload_events()
    }

    /// The global config file shown at the right of the status bar, once it is known.
    pub fn config_path(&self) -> Option<String> {
        self.cfg.config_path()
    }

    /// Status line text for the window's status bar (`#sbarMsg`).
    pub fn notice<'a>(&'a self) -> Element<'a, Message> {
        let notice_text = if self.cfg.notice.is_empty()
            || matches!(
                self.tab,
                Tab::Overview
                    | Tab::Runs
                    | Tab::Agents
                    | Tab::Tasks
                    | Tab::Patterns
                    | Tab::Monitor
                    | Tab::Snapshots
            ) {
            self.notice.clone()
        } else {
            self.cfg.notice.clone()
        };
        let notice_text = notice_text
            .trim_start_matches(crate::ui::INFO)
            .replace('\n', " ");
        let notice_color = if crate::ui::is_error(&notice_text) || notice_text.contains("✖") {
            BAD
        } else if notice_text.contains("Starting")
            || notice_text.contains("Loading")
            || notice_text.contains("confirm")
        {
            WARN
        } else if notice_text.is_empty() {
            DIM
        } else {
            OK
        };
        let shown = if notice_text.is_empty() {
            "Ready".to_owned()
        } else {
            notice_text
        };
        let color = if shown == "Ready" { DIM } else { notice_color };
        let mut line = row![
            text("■").size(8).color(color),
            container(
                text(shown)
                    .size(11)
                    .color(color)
                    .wrapping(iced::widget::text::Wrapping::None)
            )
            .max_width(620)
            .clip(true),
        ]
        .spacing(7)
        .align_y(iced::Alignment::Center);
        if !self.history.is_empty() {
            line = line.push(
                text(format!("{} notices", self.history.len()))
                    .size(11)
                    .color(theme::FAINT),
            );
        }
        line.into()
    }

    /// A list with the filter box and chips of the redesign above it.
    fn filtered_list<'a>(
        &self,
        placeholder: &'static str,
        chips: &[(&'static str, usize)],
        items: Vec<Element<'a, Message>>,
        footer: Vec<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let top = crate::ui::filter_bar(placeholder, &self.filter, chips, self.chip);
        list_box_with(Some(top), items, footer)
    }

    #[cfg(test)]
    fn update_filter_for_test(&mut self, t: &str) {
        self.filter = t.into();
    }

    /// Does this text pass the filter box (case-insensitive, every word must appear)?
    fn passes(&self, haystack: &str) -> bool {
        let h = haystack.to_lowercase();
        self.filter
            .to_lowercase()
            .split_whitespace()
            .all(|w| h.contains(w))
    }

    fn detail<'a>(&self, content: Element<'a, Message>) -> Element<'a, Message> {
        detail_box(content)
    }

    fn runs_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let active = ctx
            .data
            .runs
            .iter()
            .filter(|r| {
                matches!(
                    r.run.state,
                    PatternRunState::Running
                        | PatternRunState::Paused
                        | PatternRunState::Stalled
                        | PatternRunState::Degraded
                )
            })
            .count();
        let finished_runs = ctx
            .data
            .runs
            .iter()
            .filter(|r| {
                matches!(
                    r.run.state,
                    PatternRunState::Done | PatternRunState::Stopped
                )
            })
            .count();
        let chips = [
            ("All", ctx.data.runs.len()),
            ("Running", active),
            ("Finished", finished_runs),
        ];
        let items = ctx
            .data
            .runs
            .iter()
            .filter(|r| {
                let is_active = matches!(
                    r.run.state,
                    PatternRunState::Running
                        | PatternRunState::Paused
                        | PatternRunState::Stalled
                        | PatternRunState::Degraded
                );
                let is_finished = matches!(
                    r.run.state,
                    PatternRunState::Done | PatternRunState::Stopped
                );
                let chip_ok = match self.chip {
                    1 => is_active,
                    2 => is_finished,
                    _ => true,
                };
                chip_ok && self.passes(&format!("{} {} {}", r.run.pattern, r.run.id, r.run.task))
            })
            .map(|r| {
                let (label, color) = state_badge(r.run.state);
                let task_id = task_of_run(&r.run.task);
                let waits = ctx.data.gates.iter().any(|g| Some(g.task_id) == task_id);
                let mut body = column![
                    row![
                        text("■").size(9).color(color),
                        text(r.run.pattern.clone())
                            .font(theme::MONO_BOLD)
                            .size(13)
                            .color(theme::HI)
                            .width(Length::Fill),
                        text(label).size(11).color(color),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                    text(r.run.id.clone()).size(11).color(DIM),
                    text(match task_id.and_then(|id| ctx.data.task(id)) {
                        Some(t) => format!(
                            "#{} {}   {} of {} agents live",
                            t.status.id,
                            t.status.title,
                            r.live_agents,
                            r.run.agents.len()
                        ),
                        None => format!("{} of {} agents live", r.live_agents, r.run.agents.len()),
                    })
                    .size(11)
                    .color(DIM),
                ]
                .spacing(3);
                if waits {
                    body = body.push(
                        row![
                            text("■").size(9).color(WARN),
                            text("gate waits for you").size(11).color(WARN)
                        ]
                        .spacing(6),
                    );
                }
                button(body)
                    .width(Length::Fill)
                    .padding([4, 8])
                    .style(item_style(
                        self.run
                            .as_deref()
                            .or(ctx.data.runs.first().map(|f| f.run.id.as_str()))
                            == Some(r.run.id.as_str()),
                    ))
                    .on_press(crate::ui::nav(
                        self.run
                            .as_deref()
                            .or(ctx.data.runs.first().map(|f| f.run.id.as_str()))
                            == Some(r.run.id.as_str()),
                        Message::Panel(PanelMsg::SelectRun(r.run.id.clone())),
                    ))
                    .into()
            })
            .collect();
        let footer: Vec<Element<'a, Message>> = vec![];
        // the gate strip lives at the top of the run's own detail; the detail is the flexible region
        let detail = if self.launching {
            self.detail(self.launcher_view(ctx))
        } else {
            let (this, ctx) = (self, ctx.clone());
            crate::ui::flex(720.0, [22.0, 28.0], move || this.run_detail(&ctx))
        };
        row![
            self.filtered_list("Filter by pattern, run or task", &chips, items, footer),
            detail
        ]
        .height(Length::Fill)
        .into()
    }

    /// A gate that waits for the human (`Needs you`): what is being approved, who made it, and the buttons.
    /// Approving publishes the branch and opens the pull request; rejecting sends the reason back.
    fn gate_card<'a>(
        &'a self,
        ctx: &Ctx<'a>,
        g: &'a shepherd_app_core::data::GateRow,
    ) -> Element<'a, Message> {
        let task = ctx.data.task(g.task_id);
        let subtask = match (&g.gate, task) {
            (Gate::PushPr(n), Some(t)) => t.status.subtasks.iter().find(|s| s.number == *n),
            _ => None,
        };
        let run = ctx
            .data
            .runs
            .iter()
            .find(|r| task_of_run(&r.run.task) == Some(g.task_id));
        let title = match (&g.gate, subtask) {
            (Gate::PushPr(n), Some(s)) => {
                format!("Publish subtask {n} of task #{}: {}", g.task_id, s.title)
            }
            (Gate::PushPr(n), None) => format!("Publish subtask {n} of task #{}", g.task_id),
            (Gate::Plan, _) => format!("Approve the plan of task #{}: {}", g.task_id, g.task_title),
        };
        let what = match (&g.gate, subtask) {
            (Gate::PushPr(_), Some(s)) if !s.branch.is_empty() => format!(
                "Approving pushes branch {} and opens the pull request. Nothing is pushed until you approve.",
                s.branch
            ),
            (Gate::PushPr(_), _) => {
                "Approving pushes the branch and opens the pull request. Nothing is pushed until you approve."
                    .to_owned()
            }
            (Gate::Plan, _) => "Approving lets the coders start on the plan.".to_owned(),
        };
        let mut head = row![
            badge("awaiting-approval", WARN),
            text(title).font(theme::MONO_BOLD).size(13).color(theme::HI),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);
        if let Some(r) = run {
            head = head.push(text(format!("run {}", r.run.id)).size(12).color(ACCENT));
        }
        let mut body = column![head, text(what).size(12).color(DIM)].spacing(8);
        if let Some((files, added, removed)) = g.change {
            body = body.push(
                row![
                    text("changes").size(12).color(DIM),
                    text(format!("{files} file{}", if files == 1 { "" } else { "s" }))
                        .size(12)
                        .color(FG),
                    text(format!("+{added}")).size(12).color(OK),
                    text(format!("-{removed}")).size(12).color(BAD),
                ]
                .spacing(10),
            );
        }
        if let Some(s) = subtask {
            let mut by = String::new();
            if !s.coder.is_empty() {
                by.push_str(&format!("by {}", s.coder));
            }
            if !s.reviewer.is_empty() {
                by.push_str(&format!("   reviewed by {}", s.reviewer));
            }
            if let Some(note) = s.note.as_deref().filter(|n| !n.is_empty()) {
                by.push_str(&format!("   {note}"));
            }
            if !by.is_empty() {
                body = body.push(text(by).size(12).color(FG));
            }
        }
        body = body.push(
            row![
                self.arm_button(
                    Armed::ApproveGate(g.task_id, g.gate.clone()),
                    "Approve and open PR",
                    "Confirm: approve",
                ),
                text_input("Reason for rejecting (optional)", &self.reject_reason)
                    .on_input(|t| Message::Panel(PanelMsg::RejectReason(t)))
                    .style(theme::input_style)
                    .padding([8, 10])
                    .size(12)
                    .width(Length::Fill),
                self.arm_button(
                    Armed::RejectGate(g.task_id, g.gate.clone()),
                    "Reject",
                    "Confirm: reject",
                ),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
        alert(body.into(), WARN)
    }

    fn overview_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        // one flexible region: the page keeps at least 1000 px and scrolls when the window is narrower
        let (this, ctx) = (self, ctx.clone());
        crate::ui::flex(1000.0, [20.0, 28.0], move || this.overview_page(&ctx))
    }

    /// The Recent runs table: fixed columns (180, 140, 120, 120, 220, 140).
    fn recent_runs_table<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let d = ctx.data;
        let cell = |w: f32, e: Element<'a, Message>| container(e).width(w).clip(true);
        let head = |s: &'static str, w: f32| cell(w, text(s).size(11.5).color(DIM).into());
        let mut table = column![row![
            head("Run", 180.0),
            head("Pattern", 140.0),
            head("State", 120.0),
            head("Agents", 120.0),
            head("Task", 220.0),
            head("Started", 140.0),
        ]
        .spacing(0)]
        .spacing(10);
        for r in d.runs.iter().take(8) {
            let (state, color) = state_badge(r.run.state);
            let task = task_of_run(&r.run.task)
                .and_then(|id| d.task(id))
                .map(|t| format!("#{} {}", t.status.id, t.status.title))
                .unwrap_or_else(|| "none".to_owned());
            let task_id = task_of_run(&r.run.task);
            table = table.push(
                row![
                    cell(
                        180.0,
                        button(text(r.run.id.clone()).size(12).color(ACCENT))
                            .padding(0)
                            .style(theme::link_button_style())
                            .on_press(Message::Panel(PanelMsg::SelectRun(r.run.id.clone())))
                            .into()
                    ),
                    cell(140.0, text(r.run.pattern.clone()).size(12).into()),
                    cell(
                        120.0,
                        row![
                            text("■").size(8).color(color),
                            text(state).size(12).color(color)
                        ]
                        .spacing(6)
                        .align_y(iced::Alignment::Center)
                        .into()
                    ),
                    cell(
                        120.0,
                        text(format!("{} of {} live", r.live_agents, r.run.agents.len()))
                            .size(12)
                            .into()
                    ),
                    cell(
                        220.0,
                        match task_id {
                            Some(id) => button(text(task).size(12).color(ACCENT))
                                .padding(0)
                                .style(theme::link_button_style())
                                .on_press(Message::Panel(PanelMsg::SelectTask(id)))
                                .into(),
                            None => text(task).size(12).color(theme::FAINT).into(),
                        }
                    ),
                    cell(
                        140.0,
                        text(
                            r.run
                                .created_at
                                .chars()
                                .skip(5)
                                .take(11)
                                .collect::<String>()
                                .replace('T', " ")
                        )
                        .size(12)
                        .color(DIM)
                        .into()
                    ),
                ]
                .align_y(iced::Alignment::Center),
            );
        }
        if d.runs.is_empty() {
            table = table.push(text("No runs yet.").size(12).color(DIM));
        }
        card("RECENT RUNS", None, table.into())
    }

    fn overview_page<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let d = ctx.data;
        let running = d
            .runs
            .iter()
            .filter(|r| {
                matches!(
                    r.run.state,
                    PatternRunState::Running
                        | PatternRunState::Paused
                        | PatternRunState::Stalled
                        | PatternRunState::Degraded
                )
            })
            .count();
        let active_agents = d
            .agents
            .iter()
            .filter(|a| matches!(a.state.as_str(), "working" | "starting"))
            .count();
        let attention_runs = d
            .runs
            .iter()
            .filter(|r| {
                matches!(
                    r.run.state,
                    PatternRunState::Stalled | PatternRunState::Degraded
                )
            })
            .count();
        let is_done = |s: &str| matches!(s, "done" | "completed" | "archived");
        let open_tasks = d.tasks.iter().filter(|t| !is_done(&t.status.state)).count();
        let blocked: Vec<_> = d
            .agents
            .iter()
            .filter(|a| a.state == "blocked" || !a.notice.is_empty())
            .collect();
        let idle: Vec<_> = d.agents.iter().filter(|a| a.state == "idle").collect();
        let needs_total = d.gates.len() + blocked.len() + attention_runs;
        // each number is a link to the view it counts
        let link = |cell: Element<'a, Message>, tab: Tab| -> Element<'a, Message> {
            button(cell)
                .width(Length::FillPortion(1))
                .padding(0)
                .style(|_, status| button::Style {
                    background: msg_hot(status).then_some(Background::Color(theme::SELECTED)),
                    text_color: FG,
                    ..button::Style::default()
                })
                .on_press(Message::Panel(PanelMsg::Tab(tab)))
                .into()
        };
        // the agents' states as one segment per agent
        let agent_bar: Element<'a, Message> = {
            let mut seg = row![].spacing(2);
            for a in &d.agents {
                let c = status_color(agent_status(&a.state));
                seg = seg.push(container(text("")).width(Length::Fill).height(3).style(
                    move |_| container::Style {
                        background: Some(Background::Color(c)),
                        ..container::Style::default()
                    },
                ));
            }
            container(seg).padding([0, 16]).into()
        };
        let kpis = container(
            row![
                link(
                    kpi_cell(
                        "ACTIVE RUN",
                        running.to_string(),
                        ACCENT,
                        Some(format!("{} finished", d.runs.len() - running))
                    ),
                    Tab::Runs
                ),
                link(
                    column![
                        kpi_cell(
                            "AGENTS",
                            d.agents.len().to_string(),
                            theme::HI,
                            Some(format!(
                                "{active_agents} working, {} idle, {} blocked",
                                idle.len(),
                                blocked.len()
                            ))
                        ),
                        agent_bar
                    ]
                    .into(),
                    Tab::Agents
                ),
                link(
                    kpi_cell(
                        "OPEN TASKS",
                        open_tasks.to_string(),
                        WARN,
                        Some(if d.gates.is_empty() {
                            "nothing waits for you".to_owned()
                        } else {
                            format!("{} waits for you", d.gates.len())
                        })
                    ),
                    Tab::Tasks
                ),
                link(
                    kpi_cell(
                        "PATTERNS",
                        d.patterns.len().to_string(),
                        theme::HI,
                        Some("ready to run".into())
                    ),
                    Tab::Patterns
                ),
            ]
            .spacing(1),
        )
        .style(kpi_strip_style);

        let pulse = {
            let hot = needs_total > 0;
            let message: Element<'a, Message> = if hot {
                text(format!("{needs_total} need you"))
                    .size(12)
                    .color(WARN)
                    .into()
            } else {
                text(format!("{active_agents} agents working"))
                    .size(12)
                    .color(ACCENT)
                    .into()
            };
            container(iced::widget::stack![
                Canvas::new(Prism {
                    preset: Preset::pulse(hot),
                    frame: ctx.frame,
                })
                .width(Length::Fill)
                .height(Length::Fill),
                container(
                    container(
                        column![theme::label("HERD PULSE"), message].align_x(iced::Alignment::End)
                    )
                    .padding([6, 10])
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgba8(7, 7, 10, 0.8))),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..iced::Border::default()
                        },
                        ..container::Style::default()
                    }),
                )
                .align_right(Length::Fill)
                .align_bottom(Length::Fill)
                .padding([0, 16]),
            ])
            .height(90)
            .width(Length::Fill)
            .style(|_| container::Style {
                border: iced::Border {
                    color: LINE,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..container::Style::default()
            })
        };

        // Needs you: gates first, then blocked agents and stalled runs, then idle agents
        let needs_card = {
            let right: Element<'a, Message> = text("Nothing is pushed until you approve it")
                .size(11)
                .color(DIM)
                .into();
            let mut body = column![].spacing(10);
            for g in &d.gates {
                body = body.push(self.gate_card(ctx, g));
            }
            for a in &blocked {
                body = body.push(
                    row![
                        agent_square(agent_status(&a.state)),
                        text(a.id.clone()).size(13),
                        text(if a.notice.is_empty() {
                            "is blocked".to_owned()
                        } else {
                            a.notice.clone()
                        })
                        .size(11)
                        .color(DIM)
                        .width(Length::Fill),
                        btn("Open", PanelMsg::SelectAgent(a.id.clone())),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                );
            }
            if attention_runs > 0 {
                body = body.push(
                    text(format!("{attention_runs} run(s) are degraded or stalled."))
                        .size(12)
                        .color(WARN),
                );
            }
            if !idle.is_empty() {
                body = body.push(
                    row![
                        text(format!(
                            "{} agent(s) idle: {}",
                            idle.len(),
                            idle.iter()
                                .map(|a| a.id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                        .size(12)
                        .color(DIM)
                        .width(Length::Fill),
                        btn("Resume idle agents", PanelMsg::ResumeAgents),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                );
            }
            if needs_total == 0 && idle.is_empty() {
                body = body.push(
                    text("All clear. The flock keeps moving on its own.")
                        .size(13)
                        .color(DIM),
                );
            }
            let count: Element<'a, Message> = if needs_total > 0 {
                row![badge(needs_total, WARN), right].spacing(12).into()
            } else {
                badge("ALL CLEAR", OK)
            };
            card("NEEDS YOU", Some(count), body.into())
        };

        // Tasks: each with a segment per subtask coloured by its state
        let tasks_card = {
            let done_n = d.tasks.iter().filter(|t| is_done(&t.status.state)).count();
            let mut body = column![].spacing(12);
            for t in d.tasks.iter().take(5) {
                let subs = &t.status.subtasks;
                let mut seg = row![].spacing(3);
                let n = subs.len().max(1);
                for i in 0..n {
                    let color = match subs.get(i).map(|s| s.state.as_str()) {
                        Some("done" | "completed") => OK,
                        Some("awaiting-approval") => WARN,
                        Some("in_progress" | "in-progress" | "working" | "review") => ACCENT,
                        Some(_) => theme::LINE2,
                        None if is_done(&t.status.state) => OK,
                        None => theme::LINE2,
                    };
                    seg = seg.push(container(text("")).width(Length::Fill).height(4).style(
                        move |_| container::Style {
                            background: Some(Background::Color(color)),
                            ..container::Style::default()
                        },
                    ));
                }
                let waiting = subs
                    .iter()
                    .filter(|s| s.state == "awaiting-approval")
                    .count();
                let caption = if subs.is_empty() {
                    if is_done(&t.status.state) {
                        "Done".to_owned()
                    } else {
                        format!("Not started, {} acceptance criteria", t.criteria.len())
                    }
                } else if waiting > 0 {
                    format!("{} subtasks, {waiting} waits for your approval", subs.len())
                } else {
                    format!(
                        "{} of {} subtasks done",
                        subs.iter()
                            .filter(|s| matches!(s.state.as_str(), "done" | "completed"))
                            .count(),
                        subs.len()
                    )
                };
                body = body.push(
                    button(
                        column![
                            row![
                                text(format!("#{} {}", t.status.id, t.status.title))
                                    .font(theme::MONO_BOLD)
                                    .size(12.5)
                                    .color(theme::HI)
                                    .width(Length::Fill),
                                text(t.status.state.clone())
                                    .size(11)
                                    .color(task_state_color(&t.status.state)),
                            ],
                            seg,
                            text(caption).size(11).color(DIM),
                        ]
                        .spacing(6),
                    )
                    .width(Length::Fill)
                    .padding([6, 4])
                    .style(item_style(false))
                    .on_press(Message::Panel(PanelMsg::SelectTask(t.status.id))),
                );
            }
            if d.tasks.is_empty() {
                body = body.push(
                    column![
                        text("No tasks yet").size(13).color(theme::HI),
                        text("A task says what to build; a run gives it to a team.")
                            .size(12)
                            .color(DIM),
                        btn_primary("+ New task", PanelMsg::TaskNew),
                    ]
                    .spacing(8),
                );
            }
            let right: Element<'a, Message> = row![
                text(format!("{done_n} of {} done", d.tasks.len()))
                    .size(11)
                    .color(DIM),
                btn_link("Open tasks", PanelMsg::Tab(Tab::Tasks)),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center)
            .into();
            card("TASKS", Some(right), body.into())
        };

        // Active work: task -> run -> agents -> branches -> publish, one line per running run
        let work_card = {
            let col_head =
                |s: &'static str, w: f32| container(text(s).size(11).color(DIM)).width(w);
            let chip = |label: String, color: Color| {
                container(text(label).size(12).color(FG))
                    .padding([5, 10])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(theme::SURFACE)),
                        border: iced::Border {
                            color,
                            width: 1.0,
                            radius: 5.0.into(),
                        },
                        ..container::Style::default()
                    })
            };
            let mut body = column![row![
                col_head("Task", 130.0),
                col_head("Run", 150.0),
                col_head("Agents", 160.0),
                col_head("Branches", 150.0),
                col_head("Publish", 140.0),
            ]
            .spacing(8)]
            .spacing(10);
            let mut any = false;
            for r in d.runs.iter().filter(|r| {
                matches!(
                    r.run.state,
                    PatternRunState::Running
                        | PatternRunState::Paused
                        | PatternRunState::Stalled
                        | PatternRunState::Degraded
                )
            }) {
                any = true;
                let task_id = task_of_run(&r.run.task);
                let task_label = task_id
                    .and_then(|id| d.task(id))
                    .map(|t| format!("#{} {}", t.status.id, t.status.title))
                    .unwrap_or_else(|| "no task".to_owned());
                let agents: Vec<_> = d
                    .agents
                    .iter()
                    .filter(|a| r.run.agents.iter().any(|ra| ra.id == a.id))
                    .collect();
                let mut agent_col = column![].spacing(4);
                let mut branch_col = column![].spacing(4);
                for a in &agents {
                    let c = status_color(agent_status(&a.state));
                    agent_col = agent_col.push(
                        row![
                            text(a.id.clone()).size(12).color(FG),
                            text(a.state.clone()).size(11).color(c),
                        ]
                        .spacing(8),
                    );
                    branch_col = branch_col.push(
                        text(if a.branch.is_empty() {
                            "main".to_owned()
                        } else {
                            a.branch.clone()
                        })
                        .size(12)
                        .color(DIM),
                    );
                }
                let publish = match d.gates.iter().find(|g| Some(g.task_id) == task_id) {
                    Some(_) => chip("awaiting-approval".to_owned(), WARN),
                    None => chip("no publish step".to_owned(), theme::LINE2),
                };
                body = body.push(
                    row![
                        container(chip(task_label, theme::LINE2)).width(130),
                        container(
                            button(column![
                                text(r.run.id.clone()).size(12).color(ACCENT),
                                text(r.run.kind.clone()).size(11).color(DIM),
                            ])
                            .padding([5, 10])
                            .style(item_style(false))
                            .on_press(Message::Panel(PanelMsg::SelectRun(r.run.id.clone())))
                        )
                        .width(150),
                        container(agent_col).width(160),
                        container(branch_col).width(150),
                        container(publish).width(140),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                );
            }
            if !any {
                body = body.push(
                    column![
                        text("No run is working").size(13).color(theme::HI),
                        text("A run is a team of agents working on one task with a pattern.")
                            .size(12)
                            .color(DIM),
                        btn_primary("Run a pattern", PanelMsg::OpenLauncher(None)),
                    ]
                    .spacing(8),
                );
            }
            card(
                "ACTIVE WORK",
                Some(
                    text("How each running task reaches its agents, branches and pull requests")
                        .size(11)
                        .color(DIM)
                        .into(),
                ),
                body.into(),
            )
        };

        column![
            kpis,
            pulse,
            row![
                container(needs_card).width(Length::Fill),
                container(tasks_card).width(420)
            ]
            .spacing(20),
            work_card,
            self.recent_runs_table(ctx),
        ]
        .spacing(20)
        .into()
    }

    fn launcher_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let choices: Vec<Choice> = ctx
            .data
            .patterns
            .iter()
            .map(|p| Choice {
                name: p.name.clone(),
                label: format!("{} · {} · {} agent(s)", p.name, p.kind, p.agents_count),
            })
            .collect();
        let selected = self
            .pattern
            .as_ref()
            .and_then(|p| choices.iter().find(|c| c.name == p.name).cloned());
        let workspace = ctx
            .snapshot
            .workspaces
            .iter()
            .find(|w| w.focused)
            .map(|w| w.label.clone())
            .unwrap_or_else(|| "(no space)".into());
        let target = match self.launch_task {
            Some(id) => format!("For task #{id}. The agents join the space “{workspace}”."),
            None => format!("The agents join the space “{workspace}”."),
        };
        column![
            text("RUN PATTERN").size(10).color(DIM),
            pick_list(choices, selected, |c| Message::Panel(
                PanelMsg::PickPattern(c)
            ))
            .placeholder("Pick a pattern")
            .width(Length::Fill),
            text("Objective").size(12).color(DIM),
            text_editor(&self.objective)
                .on_action(|a| Message::Panel(PanelMsg::Edit(a)))
                .height(Length::Fixed(160.0)),
            text(target).size(11).color(DIM),
            row![
                button(container(text("Run pattern").size(13)).center_y(Length::Fill))
                    .padding([0, 16])
                    .height(32)
                    .style(primary_button_style())
                    .on_press(Message::Panel(PanelMsg::Launch)),
                button(container(text("Cancel").size(13)).center_y(Length::Fill))
                    .padding([0, 14])
                    .height(32)
                    .style(theme::outline_button_style(false))
                    .on_press(Message::Panel(PanelMsg::CloseLauncher)),
            ]
            .spacing(8),
        ]
        .spacing(8)
        .into()
    }

    fn run_detail<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let Some(r) = self
            .run
            .as_deref()
            .and_then(|id| ctx.data.runs.iter().find(|r| r.run.id == id))
            .or_else(|| ctx.data.runs.first())
        else {
            return container(
                column![
                    text("No pattern runs yet").size(16),
                    text("Launch a pattern to see its agents, health, and communication here.")
                        .size(12)
                        .color(DIM),
                    button(container(text("Run a pattern").size(13)).center_y(Length::Fill))
                        .padding([0, 12])
                        .height(32)
                        .style(primary_button_style())
                        .on_press(Message::Panel(PanelMsg::OpenLauncher(None))),
                ]
                .spacing(8),
            )
            .padding(18)
            .style(card_style)
            .into();
        };
        let run = &r.run;
        let (label, color) = state_badge(run.state);
        let live = matches!(
            run.state,
            PatternRunState::Running | PatternRunState::Stalled | PatternRunState::Degraded
        );
        // order of the spec: Communication, Pause or Resume, a divider, then the dangerous ones
        let linked_task = task_of_run(&run.task);
        let mut controls = row![btn("Communication", PanelMsg::ShowMonitor(run.id.clone()))]
            .spacing(8)
            .align_y(iced::Alignment::Center);
        if live {
            controls = controls.push(btn(
                "Pause",
                PanelMsg::Control(run.id.clone(), RunControl::Pause),
            ));
        }
        if run.state == PatternRunState::Paused {
            controls = controls.push(btn(
                "Resume",
                PanelMsg::Control(run.id.clone(), RunControl::Resume),
            ));
        }
        controls =
            controls.push(
                container(text(""))
                    .width(1)
                    .height(22)
                    .style(|_| container::Style {
                        background: Some(Background::Color(theme::LINE2)),
                        ..container::Style::default()
                    }),
            );
        if live || run.state == PatternRunState::Paused {
            controls = controls.push(self.arm_button(
                Armed::StopRun(run.id.clone()),
                "Stop",
                format!("Confirm: stop and close {} agents", r.live_agents),
            ));
            if let Some(t) = linked_task.and_then(|id| ctx.data.task(id)) {
                if has_run(&t.status.state) {
                    controls = controls.push(self.arm_button(
                        Armed::CancelRun(t.status.id),
                        "Cancel task's run",
                        format!("Confirm: cancel task #{}'s run", t.status.id),
                    ));
                }
            }
        }
        controls = controls.push(self.arm_button(
            Armed::DeleteRun(run.id.clone()),
            "Delete run",
            format!("Confirm: delete {}", run.id),
        ));
        let mut col = column![
            row![
                text(run.pattern.clone())
                    .font(theme::MONO_BOLD)
                    .size(22)
                    .color(theme::HI),
                text(format!("run {}", run.id)).size(12).color(DIM),
                badge(label, color),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            text(format!(
                "topology {}   started {}   updated {}",
                run.kind, run.created_at, run.updated_at
            ))
            .size(12)
            .color(DIM),
            controls,
        ]
        .spacing(14);
        if let Some(reason) = run.stalled_reason.as_deref() {
            col = col.push(alert(
                text(format!("STALLED  {reason}"))
                    .size(13)
                    .color(WARN)
                    .into(),
                WARN,
            ));
        }
        if let Some(error) = run.error.as_deref() {
            col = col.push(alert(
                text(format!("ERROR  {error}")).size(13).color(BAD).into(),
                BAD,
            ));
        }
        for g in ctx
            .data
            .gates
            .iter()
            .filter(|g| Some(g.task_id) == linked_task)
        {
            col = col.push(self.gate_card(ctx, g));
        }
        col = col.push(self.run_chain(ctx, r));
        col = col.push(self.topology_card(ctx, r));
        let mut objective = column![text(if run.task.is_empty() {
            "No objective.".to_owned()
        } else {
            run.task.clone()
        })
        .size(13)]
        .spacing(8);
        if let Some(worktree) = &run.worktree {
            objective = objective.push(text(format!("Worktree: {worktree}")).size(11).color(DIM));
        }
        if !run.topics.is_empty() {
            objective = objective.push(
                text(format!("Topics: {}", run.topics.join(", ")))
                    .size(11)
                    .color(DIM),
            );
        }
        let linked = task_of_run(&run.task).map(|task| {
            btn_link(
                format!("Linked to task #{task}"),
                PanelMsg::SelectTask(task),
            )
        });
        if let Some(t) = linked_task.and_then(|id| ctx.data.task(id)) {
            for s in &t.status.subtasks {
                let who = if s.coder.is_empty() {
                    "queued".to_owned()
                } else {
                    s.coder.clone()
                };
                let c = match s.state.as_str() {
                    "awaiting-approval" => WARN,
                    "done" | "completed" => OK,
                    "in_progress" | "in-progress" | "working" => ACCENT,
                    _ => DIM,
                };
                objective = objective.push(
                    row![
                        text(s.number.to_string()).size(11).color(DIM).width(16),
                        text(s.title.clone()).size(12).width(Length::Fill),
                        text(who).size(11).color(DIM),
                        text(s.state.clone()).size(11).color(c),
                    ]
                    .spacing(8),
                );
            }
        }
        let objective_card = card("OBJECTIVE", linked, objective.into());
        let mut agents = column![].spacing(6);
        for a in &run.agents {
            let status = agent_status(&a.state);
            agents = agents.push(
                row![
                    agent_square(status),
                    text(a.id.clone()).size(13).color(theme::HI),
                    text(format!("({})", a.role)).size(13).color(DIM),
                    text(format!("[{}]", a.state))
                        .size(13)
                        .color(status_color(status)),
                    text(a.vendor.clone().unwrap_or_default())
                        .size(13)
                        .color(DIM),
                    container(text("")).width(Length::Fill),
                    btn_link("open", PanelMsg::SelectAgent(a.id.clone())),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
            );
        }
        col = col.push(
            row![
                container(objective_card).width(Length::Fill),
                container(card("AGENTS", None, agents.into())).width(Length::Fill),
            ]
            .spacing(14),
        );
        col.into()
    }

    /// The pills that follow one piece of work: task > run > agents > branches > publish.
    fn run_chain<'a>(
        &'a self,
        ctx: &Ctx<'a>,
        r: &'a shepherd_app_core::data::RunRow,
    ) -> Element<'a, Message> {
        let d = ctx.data;
        let task_id = task_of_run(&r.run.task);
        let agents: Vec<_> = d
            .agents
            .iter()
            .filter(|a| r.run.agents.iter().any(|ra| ra.id == a.id))
            .collect();
        let mut branches: Vec<String> = vec![];
        for a in &agents {
            let b = if a.branch.is_empty() {
                "main".to_owned()
            } else {
                a.branch.clone()
            };
            if !branches.contains(&b) {
                branches.push(b);
            }
        }
        let waits = d.gates.iter().any(|g| Some(g.task_id) == task_id);
        let pill = |key: &'static str, value: String, color: Color, msg: Option<PanelMsg>| {
            let body = row![
                text(key).size(11).color(DIM),
                text(value).size(12).color(color),
            ]
            .spacing(8);
            let b = button(body).padding([4, 12]).style(move |_, status| {
                let hot = msg_hot(status);
                button::Style {
                    background: hot.then_some(Background::Color(theme::SURFACE)),
                    text_color: FG,
                    border: iced::Border {
                        color: if color == ACCENT {
                            ACCENT
                        } else {
                            theme::LINE2
                        },
                        width: 1.0,
                        radius: 14.0.into(),
                    },
                    ..button::Style::default()
                }
            });
            match msg {
                Some(m) => b.on_press(Message::Panel(m)),
                None => b,
            }
        };
        let arrow = || text("›").size(14).color(DIM);
        let mut r_ = row![].spacing(8).align_y(iced::Alignment::Center);
        if let Some(id) = task_id {
            let title = d
                .task(id)
                .map(|t| t.status.title.clone())
                .unwrap_or_default();
            r_ = r_
                .push(pill(
                    "task",
                    format!("#{id} {title}"),
                    FG,
                    Some(PanelMsg::SelectTask(id)),
                ))
                .push(arrow());
        }
        r_ = r_
            .push(pill("run", r.run.id.clone(), ACCENT, None))
            .push(arrow())
            .push(pill(
                "agents",
                agents.len().max(r.run.agents.len()).to_string(),
                FG,
                Some(PanelMsg::Tab(Tab::Agents)),
            ))
            .push(arrow())
            .push(pill(
                "branches",
                branches.join(", "),
                FG,
                Some(PanelMsg::Tab(Tab::Git)),
            ))
            .push(arrow())
            .push(pill(
                "publish",
                if waits {
                    "waits for you".to_owned()
                } else {
                    "no publish step".to_owned()
                },
                if waits { WARN } else { DIM },
                None,
            ));
        r_.wrap().into()
    }

    /// `Live topology`: who publishes to which topic and who reads it.
    fn topology_card<'a>(
        &'a self,
        ctx: &Ctx<'a>,
        r: &'a shepherd_app_core::data::RunRow,
    ) -> Element<'a, Message> {
        let legend = |color: Color, name: &'static str| {
            row![
                text("■").size(10).color(color),
                text(name).size(11).color(DIM)
            ]
            .spacing(4)
        };
        let right: Element<'a, Message> = row![
            legend(ACCENT, "working"),
            legend(theme::IDLE, "idle"),
            legend(WARN, "blocked"),
            legend(theme::VIOLET, "topic"),
        ]
        .spacing(12)
        .into();
        let body: Element<'a, Message> = match &self.run_graph {
            Some((_, graph)) if !graph.nodes.is_empty() => {
                let agents = r
                    .run
                    .agents
                    .iter()
                    .map(|a| {
                        let live = ctx.data.agents.iter().find(|x| x.id == a.id);
                        let state = live.map_or(a.state.as_str(), |x| x.state.as_str());
                        let color = status_color(agent_status(state));
                        let word = if state == "blocked" {
                            "needs you"
                        } else {
                            state
                        };
                        (a.id.clone(), a.role.clone(), color, word.to_owned())
                    })
                    .collect();
                Canvas::new(crate::graph_view::TopologyView {
                    graph,
                    agents,
                    live: matches!(r.run.state, PatternRunState::Running),
                    frame: ctx.frame,
                })
                .width(Length::Fill)
                .height(graph.height + 40.0)
                .into()
            }
            _ => text(
                "The pattern of this run could not be loaded, so there is no topology to draw.",
            )
            .size(12)
            .color(DIM)
            .into(),
        };
        card("LIVE TOPOLOGY", Some(right), body)
    }

    fn agents_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let working = ctx
            .data
            .agents
            .iter()
            .filter(|a| matches!(a.state.as_str(), "working" | "starting"))
            .count();
        let needs_you = ctx
            .data
            .agents
            .iter()
            .filter(|a| a.state == "blocked" || !a.notice.is_empty())
            .count();
        let finished = ctx
            .data
            .agents
            .iter()
            .filter(|a| matches!(a.state.as_str(), "done" | "exited" | "killed" | "failed"))
            .count();
        let selected = self
            .agent
            .as_deref()
            .and_then(|id| ctx.data.agents.iter().find(|a| a.id == id))
            .or_else(|| ctx.data.agents.first())
            .map(|a| a.id.clone());
        let idle = ctx.data.agents.iter().filter(|a| a.state == "idle").count();
        let chips = [
            ("All", ctx.data.agents.len()),
            ("Working", working),
            ("Idle", idle),
            ("Blocked", needs_you),
            ("Done", finished),
        ];
        let items: Vec<Element<'a, Message>> = ctx
            .data
            .agents
            .iter()
            .filter(|a| {
                let chip_ok = match self.chip {
                    1 => matches!(a.state.as_str(), "working" | "starting"),
                    2 => a.state == "idle",
                    3 => a.state == "blocked" || !a.notice.is_empty(),
                    4 => matches!(a.state.as_str(), "done" | "exited" | "killed" | "failed"),
                    _ => true,
                };
                chip_ok && self.passes(&format!("{} {} {} {}", a.id, a.role, a.vendor, a.model))
            })
            .map(|a| {
                let status = agent_status(&a.state);
                let needs_you = a.state == "blocked" || !a.notice.is_empty();
                let (state_word, state_color) = if needs_you {
                    ("needs you", WARN)
                } else {
                    (a.state.as_str(), status_color(status))
                };
                let selected_card = selected.as_deref() == Some(a.id.as_str());
                let empty = tail_is_empty(&self.outputs, &a.pane);
                let tail: String = self
                    .outputs
                    .get(&a.pane)
                    .map(|t| {
                        let lines: Vec<&str> = t.lines().filter(|l| !l.trim().is_empty()).collect();
                        let from = lines.len().saturating_sub(3);
                        lines[from..]
                            .iter()
                            .map(|l| l.chars().take(46).collect::<String>())
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                let output = container(
                    text(if empty {
                        "No output yet".to_owned()
                    } else {
                        tail
                    })
                    .size(11)
                    .color(if empty { DIM } else { FG }),
                )
                .padding([8, 10])
                .width(Length::Fill)
                .height(64)
                .style(|_| container::Style {
                    background: Some(Background::Color(theme::BG)),
                    border: iced::Border {
                        color: LINE,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..container::Style::default()
                });
                let body = column![
                    row![
                        agent_square(status),
                        column![
                            text(a.id.clone())
                                .font(theme::MONO_BOLD)
                                .size(13)
                                .color(theme::HI),
                            text(format!("{}, {} {}", a.role, a.vendor, a.model))
                                .size(11)
                                .color(DIM),
                        ]
                        .spacing(1)
                        .width(Length::Fill),
                        container(text(state_word).size(10.5).color(state_color))
                            .padding([1, 7])
                            .style(move |_| container::Style {
                                border: iced::Border {
                                    color: state_color,
                                    width: 1.0,
                                    radius: 3.0.into(),
                                },
                                ..container::Style::default()
                            }),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                    text(if a.task_text.is_empty() {
                        "No task".to_owned()
                    } else {
                        a.task_text.chars().take(52).collect()
                    })
                    .size(12)
                    .color(FG),
                    output,
                    row![
                        text(if a.branch.is_empty() {
                            "main".to_owned()
                        } else {
                            a.branch.clone()
                        })
                        .size(11)
                        .color(DIM)
                        .width(Length::Fill),
                        text(if a.pane.is_empty() {
                            String::new()
                        } else {
                            format!("pane {}", a.pane)
                        })
                        .size(11)
                        .color(DIM),
                    ],
                ]
                .spacing(8);
                button(body)
                    .width(300)
                    .height(Length::Shrink)
                    .padding([12, 12])
                    .style(move |_, status| {
                        let hot = msg_hot(status);
                        button::Style {
                            background: Some(Background::Color(if selected_card || hot {
                                theme::SELECTED
                            } else {
                                theme::SURFACE
                            })),
                            text_color: FG,
                            border: iced::Border {
                                color: if selected_card { ACCENT } else { LINE },
                                width: 1.0,
                                radius: 6.0.into(),
                            },
                            ..button::Style::default()
                        }
                    })
                    .on_press(Message::Panel(PanelMsg::SelectAgent(a.id.clone())))
                    .into()
            })
            .collect();
        let footer: Vec<Element<'a, Message>> = vec![];
        let detail: Element<'a, Message> = if self.spawning_agent {
            self.spawn_agent_view(ctx)
        } else {
            match ctx
                .data
                .agents
                .iter()
                .find(|a| Some(a.id.as_str()) == selected.as_deref())
            {
                None => text("No agents. Launch one with \"+ New agent\".")
                    .color(DIM)
                    .into(),
                Some(a) => self.agent_detail(ctx, a),
            }
        };
        // cards on the left (two per row), the selected agent on the right
        let grid = row(items).spacing(14).wrap().vertical_spacing(14);
        let left = column![
            crate::ui::filter_bar(
                "Filter by name, role, vendor or task",
                &self.filter,
                &chips,
                self.chip
            ),
            crate::theme::scroll(container(grid).padding([4, 12])).height(Length::Fill),
        ]
        .width(Length::Fill);
        let _ = footer;
        column![row![
            left,
            container(text(""))
                .width(1)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(LINE)),
                    ..container::Style::default()
                }),
            container(self.detail(detail))
                .width(460)
                .height(Length::Fill)
                .style(theme::inspector_style),
        ]
        .height(Length::Fill),]
        .spacing(8)
        .into()
    }

    /// Form for launching a single agent with a Shepherd role.
    /// The detail of one agent (`V.agents`): header, actions, live output, approval notice and context cards.
    fn agent_detail<'a>(
        &'a self,
        ctx: &Ctx<'a>,
        a: &'a shepherd_app_core::shepherd_core::Agent,
    ) -> Element<'a, Message> {
        let status = agent_status(&a.state);
        let linked_run = ctx
            .data
            .runs
            .iter()
            .find(|run| run.run.agents.iter().any(|ra| ra.id == a.id));
        let mut sub = format!("{} · {} {}", a.role, a.vendor, a.model);
        if let Some(run) = linked_run {
            sub.push_str(&format!(" · run {}", run.run.pattern));
        }
        let action = |label: &'static str, msg: Message, primary: bool| -> Element<'a, Message> {
            button(container(text(label).size(13)).center_y(Length::Fill))
                .padding([0, 12])
                .height(32)
                .style(if primary {
                    Box::new(primary_button_style())
                        as Box<dyn Fn(&iced::Theme, button::Status) -> button::Style>
                } else {
                    Box::new(theme::outline_button_style(false))
                })
                .on_press(msg)
                .into()
        };
        let mut actions = row![].spacing(8);
        if !a.pane.is_empty() {
            actions = actions.push(action("Terminal", Message::GoToPane(a.pane.clone()), true));
        }
        actions = actions.push(action(
            "Focus agent",
            Message::Act(Action::FocusAgent(a.id.clone())),
            false,
        ));
        if !a.workspace.is_empty() {
            actions = actions.push(action(
                "Workspace",
                Message::Act(Action::FocusSpace(a.workspace.clone())),
                false,
            ));
        }
        if !a.worktree.is_empty() || !a.branch.is_empty() {
            actions = actions.push(btn("Git", PanelMsg::Tab(Tab::Git)));
        }
        if !["done", "exited", "killed", "failed"]
            .iter()
            .any(|s| a.state == *s)
        {
            actions = actions.push(
                button(container(text("Close agent").size(13)).center_y(Length::Fill))
                    .padding([0, 12])
                    .height(32)
                    .style(danger_button_style())
                    .on_press(Message::Panel(PanelMsg::KillAgent(a.id.clone()))),
            );
        }
        let mut col = column![
            row![
                text(a.id.clone()).size(18).color(theme::HI),
                text(sub).size(12).color(DIM),
                badge(a.state.to_uppercase(), status_color(status)),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            actions.wrap(),
        ]
        .spacing(14);

        // Live output: the latest messages of this agent (the terminal itself lives in its pane).
        let mut output = column![].spacing(6);
        let recent: Vec<_> = self.feed.iter().rev().take(6).map(feed_line).collect();
        if recent.is_empty() {
            output = output.push(
                text("No recent communication for this agent.")
                    .size(12)
                    .color(DIM),
            );
        }
        for (head, body) in recent {
            output = output
                .push(text(head).size(11).color(ACCENT))
                .push(text(body).size(12.5));
        }
        let pane =
            (!a.pane.is_empty()).then(|| text(format!("pane {}", a.pane)).size(11).color(DIM));
        col = col.push(card("LIVE OUTPUT", pane.map(Element::from), output.into()));

        if !a.notice.is_empty() {
            let body = column![text(a.notice.clone()).size(13).color(theme::HI)].spacing(8);
            col = col.push(
                container(column![
                    container(theme::label("APPROVAL REQUIRED").color(WARN)).padding([10, 14]),
                    container(body).padding(pad(4.0, 14.0, 12.0, 14.0)),
                ])
                .width(Length::Fill)
                .style(theme::alert_style(WARN)),
            );
        }
        if !a.task_text.is_empty() {
            col = col.push(card(
                "TASK",
                None,
                text(a.task_text.clone()).size(13).into(),
            ));
        }
        col = col.push(card(
            "SKILLS",
            None,
            column(
                agent_skill_lines(a)
                    .into_iter()
                    .map(|line| text(line).size(12.5).into()),
            )
            .push(column(agent_skill_buttons(a)).spacing(2))
            .spacing(4)
            .into(),
        ));
        if !a.result.is_empty() {
            col = col.push(card("RESULT", None, text(a.result.clone()).size(13).into()));
        }
        if !a.worktree.is_empty() || !a.branch.is_empty() {
            col = col.push(card(
                "SOURCE CONTROL",
                None,
                column![
                    text(if a.branch.is_empty() {
                        "detached HEAD".to_owned()
                    } else {
                        format!("branch {}", a.branch)
                    })
                    .size(13)
                    .color(ACCENT),
                    text(if a.worktree.is_empty() {
                        "No dedicated worktree".to_owned()
                    } else {
                        a.worktree.clone()
                    })
                    .size(11)
                    .color(DIM),
                ]
                .spacing(4)
                .into(),
            ));
        }
        if let Some(run) = linked_run {
            let (run_state, run_color) = state_badge(run.run.state);
            col = col.push(card(
                "RUN CONTEXT",
                Some(btn_link(
                    "Open communication",
                    PanelMsg::ShowMonitor(run.run.id.clone()),
                )),
                column![
                    text(format!("{} · {}", run.run.pattern, run_state))
                        .size(13)
                        .color(run_color),
                    text(if run.run.task.is_empty() {
                        "No objective.".to_owned()
                    } else {
                        run.run.task.clone()
                    })
                    .size(12),
                ]
                .spacing(4)
                .into(),
            ));
        }
        if !ctx.data.tasks.is_empty() {
            let task_choices: Vec<Choice> = ctx
                .data
                .tasks
                .iter()
                .map(|t| Choice {
                    name: t.status.id.to_string(),
                    label: format!("#{} {}", t.status.id, t.status.title),
                })
                .collect();
            let current = task_choices.iter().find(|c| c.name == a.task_id).cloned();
            col = col.push(card(
                "ASSIGN TO TASK",
                None,
                pick_list(task_choices, current, |c| {
                    Message::Panel(PanelMsg::AssignAgentTask(
                        a.id.clone(),
                        Some(c.name.parse().unwrap_or(0)),
                    ))
                })
                .placeholder("(none)")
                .width(Length::Fill)
                .into(),
            ));
        }
        container(col).max_width(980).into()
    }

    fn spawn_agent_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let role_choices: Vec<Choice> = ctx
            .data
            .models
            .roles
            .iter()
            .map(|(name, role)| Choice {
                name: name.clone(),
                label: format!(
                    "{} · {}{}",
                    name,
                    role.vendor,
                    if !role.model.is_empty() {
                        format!(" / {}", role.model)
                    } else {
                        String::new()
                    }
                ),
            })
            .collect();
        let selected_role = self
            .spawn_role
            .as_ref()
            .and_then(|r| role_choices.iter().find(|c| c.name == r.name).cloned());
        let workspace_choices: Vec<Choice> = ctx
            .snapshot
            .workspaces
            .iter()
            .map(|w| Choice {
                name: w.workspace_id.clone(),
                label: w.label.clone(),
            })
            .collect();
        let selected_workspace = self
            .spawn_workspace
            .as_ref()
            .and_then(|w| workspace_choices.iter().find(|c| c.name == w.name).cloned());
        let vendor_choices: Vec<Choice> = ctx
            .data
            .models
            .vendors
            .iter()
            .map(|(name, v)| Choice {
                name: name.clone(),
                label: format!(
                    "{}{}",
                    name,
                    if !v.command.is_empty() {
                        " ✓"
                    } else {
                        " — not installed"
                    }
                ),
            })
            .collect();
        let selected_vendor = self
            .spawn_vendor
            .as_ref()
            .and_then(|v| vendor_choices.iter().find(|c| c.name == v.name).cloned());
        let workspace_label = self
            .spawn_workspace
            .as_ref()
            .map(|w| w.label.clone())
            .unwrap_or_else(|| "(no space)".into());
        column![
            text("LAUNCH AGENT").size(10).color(DIM),
            text("Role").size(12).color(DIM),
            pick_list(role_choices, selected_role, |c| Message::Panel(
                PanelMsg::SpawnAgentRole(c)
            ))
            .placeholder("Pick a role")
            .width(Length::Fill),
            text("Workspace").size(12).color(DIM),
            pick_list(workspace_choices, selected_workspace, |c| Message::Panel(
                PanelMsg::SpawnAgentWorkspace(c)
            ))
            .placeholder("Pick a workspace")
            .width(Length::Fill),
            text("Vendor (optional, from role)").size(12).color(DIM),
            pick_list(vendor_choices, selected_vendor, |c| Message::Panel(
                PanelMsg::SpawnAgentVendor(c)
            ))
            .placeholder("Pick a vendor")
            .width(Length::Fill),
            text("Model (optional)").size(12).color(DIM),
            text_input("Model name", &self.spawn_model)
                .on_input(|v| Message::Panel(PanelMsg::SpawnAgentModel(v)))
                .width(Length::Fill),
            text("Task").size(12).color(DIM),
            text_input("Agent task description", &self.spawn_task_text)
                .on_input(|v| Message::Panel(PanelMsg::SpawnAgentTask(v)))
                .width(Length::Fill),
            checkbox(self.spawn_worktree)
                .label("in its own worktree")
                .on_toggle(|on| Message::Panel(PanelMsg::SpawnAgentWorktree(on)))
                .text_size(12),
            text(format!("The agent joins the space \"{workspace_label}\"."))
                .size(11)
                .color(DIM),
            row![
                button(container(text("Launch").size(13)).center_y(Length::Fill))
                    .padding([0, 16])
                    .height(32)
                    .style(primary_button_style())
                    .on_press(Message::Panel(PanelMsg::SpawnAgent)),
                button(container(text("Cancel").size(13)).center_y(Length::Fill))
                    .padding([0, 14])
                    .height(32)
                    .style(theme::outline_button_style(false))
                    .on_press(Message::Panel(PanelMsg::CloseSpawnAgent)),
            ]
            .spacing(8),
        ]
        .spacing(8)
        .into()
    }

    /// The board of Tasks: New, In run and Done share the width (at least 220 each), Cancelled is 128 wide.
    fn tasks_board<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let d = ctx.data;
        let is_done = |state: &str| matches!(state, "done" | "completed" | "archived");
        let selected = self
            .task
            .and_then(|id| d.task(id))
            .or_else(|| d.tasks.first())
            .map(|t| t.status.id);
        let met = |t: &shepherd_app_core::data::TaskRow| {
            t.criteria
                .iter()
                .filter(|c| c.trim_start().starts_with("- [x]"))
                .count()
        };
        // one card per task: its subtasks as segments, how many criteria are met, what waits, its run
        let card_of = |t: &'a shepherd_app_core::data::TaskRow| -> Element<'a, Message> {
            let subs = &t.status.subtasks;
            let sel = selected == Some(t.status.id);
            let mut body = column![row![
                text(format!("#{}", t.status.id)).size(11).color(DIM),
                text(t.status.title.clone())
                    .font(theme::MONO_BOLD)
                    .size(13)
                    .color(theme::HI),
            ]
            .spacing(8)]
            .spacing(8);
            if !subs.is_empty() {
                let mut seg = row![].spacing(3);
                for s in subs {
                    let color = match s.state.as_str() {
                        "done" | "completed" => OK,
                        "awaiting-approval" => WARN,
                        "in_progress" | "in-progress" | "working" | "review" => ACCENT,
                        _ => theme::LINE2,
                    };
                    seg = seg.push(container(text("")).width(Length::Fill).height(3).style(
                        move |_| container::Style {
                            background: Some(Background::Color(color)),
                            ..container::Style::default()
                        },
                    ));
                }
                body = body.push(seg);
            }
            let crit = format!("{} of {} criteria met", met(t), t.criteria.len());
            body = body.push(
                text(if subs.is_empty() {
                    crit
                } else {
                    format!("{} subtasks, {crit}", subs.len())
                })
                .size(11)
                .color(DIM),
            );
            let waiting = subs
                .iter()
                .filter(|s| s.state == "awaiting-approval")
                .count();
            if waiting > 0 {
                body = body.push(
                    row![
                        text("■").size(9).color(WARN),
                        text(format!("{waiting} subtask waits for you"))
                            .size(11)
                            .color(WARN)
                    ]
                    .spacing(6),
                );
            }
            if let Some(r) = d
                .runs
                .iter()
                .find(|r| task_of_run(&r.run.task) == Some(t.status.id))
            {
                body = body.push(text(format!("run {}", r.run.id)).size(11).color(ACCENT));
            }
            button(body)
                .width(Length::Fill)
                .padding([10, 12])
                .style(move |_, status| {
                    let hot = msg_hot(status);
                    button::Style {
                        background: Some(Background::Color(if sel || hot {
                            theme::SELECTED
                        } else {
                            theme::SURFACE
                        })),
                        text_color: FG,
                        border: iced::Border {
                            color: if sel { ACCENT } else { LINE },
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..button::Style::default()
                    }
                })
                .on_press(Message::Panel(PanelMsg::SelectTask(t.status.id)))
                .into()
        };
        let column_of = |title: &'static str,
                         color: Color,
                         tasks: Vec<&'a shepherd_app_core::data::TaskRow>,
                         extra: Option<Element<'a, Message>>|
         -> Element<'a, Message> {
            let mut head = row![
                text("■").size(10).color(color),
                text(title)
                    .font(theme::MONO_BOLD)
                    .size(12.5)
                    .color(theme::HI),
                text(tasks.len().to_string()).size(11).color(DIM),
                container(text("")).width(Length::Fill),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center);
            if let Some(e) = extra {
                head = head.push(e);
            }
            let mut cards = column![].spacing(8);
            if tasks.is_empty() {
                cards = cards.push(text("None.").size(12).color(DIM));
            }
            for t in tasks {
                cards = cards.push(card_of(t));
            }
            container(column![head, crate::theme::scroll(cards).height(Length::Fill)].spacing(10))
                .padding(12)
                .width(if title == "Cancelled" {
                    Length::Fixed(128.0)
                } else {
                    Length::FillPortion(1)
                })
                .height(Length::Fill)
                .style(card_style)
                .into()
        };
        let shown = |t: &&shepherd_app_core::data::TaskRow| {
            self.passes(&format!("#{} {}", t.status.id, t.status.title))
        };
        let linked = |t: &&shepherd_app_core::data::TaskRow| {
            d.runs
                .iter()
                .any(|r| task_of_run(&r.run.task) == Some(t.status.id))
        };
        let in_run = |t: &&shepherd_app_core::data::TaskRow| {
            has_run(&t.status.state) || (t.status.state == "new" && linked(t))
        };
        let new_tasks: Vec<_> = d
            .tasks
            .iter()
            .filter(|t| t.status.state == "new" && !linked(t))
            .filter(shown)
            .collect();
        let running: Vec<_> = d
            .tasks
            .iter()
            .filter(|t| in_run(t) && t.status.state != "cancelled")
            .filter(shown)
            .collect();
        let done: Vec<_> = d
            .tasks
            .iter()
            .filter(|t| is_done(&t.status.state))
            .filter(shown)
            .collect();
        let cancelled: Vec<_> = d
            .tasks
            .iter()
            .filter(|t| t.status.state == "cancelled")
            .filter(shown)
            .collect();
        row![
            column_of("New", theme::IDLE, new_tasks, None),
            column_of("In run", ACCENT, running, None),
            column_of(
                "Done",
                OK,
                done,
                Some(self.arm_button(Armed::ArchiveDone, "Archive", "Confirm: archive"))
            ),
            column_of("Cancelled", BAD, cancelled, None),
        ]
        .spacing(14)
        .height(Length::Fixed(520.0))
        .into()
    }

    fn tasks_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let d = ctx.data;
        let selected = self
            .task
            .and_then(|id| d.task(id))
            .or_else(|| d.tasks.first())
            .map(|t| t.status.id);
        let met = |t: &shepherd_app_core::data::TaskRow| {
            t.criteria
                .iter()
                .filter(|c| c.trim_start().starts_with("- [x]"))
                .count()
        };
        let board = crate::ui::flex(878.0, [16.0, 28.0], {
            let (this, ctx) = (self, ctx.clone());
            move || this.tasks_board(&ctx)
        });
        let detail: Element<'a, Message> =
            match d.tasks.iter().find(|t| Some(t.status.id) == selected) {
                None => column![
                    text("No task selected").size(13).color(theme::HI),
                    text(format!(
                        "Tasks of {} show up on the board.",
                        d.root.display()
                    ))
                    .size(12)
                    .color(DIM),
                    btn_primary("+ New task", PanelMsg::TaskNew),
                ]
                .spacing(8)
                .into(),
                Some(t) => {
                    let runs: Vec<_> = d
                        .runs
                        .iter()
                        .filter(|r| task_of_run(&r.run.task) == Some(t.status.id))
                        .collect();
                    let state_color = task_state_color(&t.status.state);
                    let mut col = column![row![
                        text(format!("#{}", t.status.id)).size(13).color(DIM),
                        text(t.status.title.clone())
                            .font(theme::MONO_BOLD)
                            .size(20)
                            .color(theme::HI),
                        badge(t.status.state.clone(), state_color),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),]
                    .spacing(14);
                    let mut actions = row![].spacing(8).align_y(iced::Alignment::Center);
                    match runs.first() {
                        Some(r) => {
                            actions = actions.push(btn_primary(
                                "Open run",
                                PanelMsg::SelectRun(r.run.id.clone()),
                            ))
                        }
                        None => {
                            actions = actions.push(btn_primary(
                                "Run with an agent",
                                PanelMsg::RunTaskWithAgent(t.status.id),
                            ))
                        }
                    }
                    if !runs.is_empty() {
                        actions = actions.push(btn(
                            "Run with an agent",
                            PanelMsg::RunTaskWithAgent(t.status.id),
                        ));
                    }
                    actions = actions
                        .push(btn(
                            "Run with a pattern",
                            PanelMsg::OpenLauncher(Some(t.status.id)),
                        ))
                        .push(btn("Edit", PanelMsg::TaskEditOpen(t.status.id)));
                    if has_run(&t.status.state) {
                        actions = actions.push(self.arm_button(
                            Armed::CancelRun(t.status.id),
                            "Cancel run",
                            "Confirm: cancel the run",
                        ));
                    }
                    actions = actions.push(self.arm_button(
                        Armed::DeleteTask(t.status.id),
                        "Delete",
                        "Confirm: delete for good",
                    ));
                    col = col.push(actions.wrap());
                    if !t.description.is_empty() {
                        col = col.push(
                            column![
                                theme::label("SPECIFICATION"),
                                text(t.description.clone()).size(13)
                            ]
                            .spacing(8),
                        );
                    }
                    if !t.criteria.is_empty() {
                        let mut rows = column![row![
                            theme::label("ACCEPTANCE CRITERIA"),
                            text(format!("{} of {} met", met(t), t.criteria.len()))
                                .size(11)
                                .color(DIM)
                        ]
                        .spacing(10)]
                        .spacing(8);
                        for c in &t.criteria {
                            let ok = c.trim_start().starts_with("- [x]");
                            let text_of = c
                                .trim_start_matches("- [ ] ")
                                .trim_start_matches("- [x] ")
                                .trim_start_matches("- ")
                                .to_owned();
                            rows = rows.push(
                                row![
                                    container(
                                        text(if ok { "✓" } else { "" }).size(11).color(theme::BG)
                                    )
                                    .width(16)
                                    .height(16)
                                    .center_x(16)
                                    .center_y(16)
                                    .style(move |_| {
                                        container::Style {
                                            background: ok.then_some(Background::Color(OK)),
                                            border: iced::Border {
                                                color: if ok { OK } else { theme::LINE2 },
                                                width: 1.0,
                                                radius: 3.0.into(),
                                            },
                                            ..container::Style::default()
                                        }
                                    }),
                                    text(text_of).size(13).width(Length::Fill),
                                ]
                                .spacing(10),
                            );
                        }
                        col = col.push(rows);
                    }
                    if !t.status.subtasks.is_empty() {
                        let mut rows = column![theme::label("SUBTASKS")].spacing(0);
                        for s in &t.status.subtasks {
                            let state = s.state.as_str();
                            let color = match state {
                                "awaiting-approval" => WARN,
                                "done" | "completed" => OK,
                                "in_progress" | "in-progress" | "working" => ACCENT,
                                _ => DIM,
                            };
                            let mut line = row![
                                text(s.number.to_string()).size(11).color(DIM).width(18),
                                text(s.title.clone())
                                    .size(12.5)
                                    .color(theme::HI)
                                    .width(Length::Fill),
                            ]
                            .spacing(8)
                            .align_y(iced::Alignment::Center);
                            if state == "awaiting-approval" {
                                if let Some(r) = runs.first() {
                                    line = line.push(btn_link(
                                        "Review gate",
                                        PanelMsg::SelectRun(r.run.id.clone()),
                                    ));
                                }
                            }
                            line = line.push(text(state.to_owned()).size(11).color(color));
                            rows = rows.push(container(line).padding([8, 12]).width(Length::Fill));
                        }
                        col = col.push(
                            container(rows)
                                .padding(pad(10.0, 0.0, 4.0, 0.0))
                                .style(card_style),
                        );
                    }
                    // where the work lives: run, agents, branches, source
                    let mut lives = column![theme::label("WHERE IT LIVES")].spacing(6);
                    let kv = |k: &'static str, v: String| {
                        row![
                            text(k).size(11.5).color(DIM).width(70),
                            text(v).size(12).color(FG)
                        ]
                        .spacing(8)
                    };
                    if let Some(r) = runs.first() {
                        lives = lives.push(kv(
                            "run",
                            format!("{} ({})", r.run.id, state_badge(r.run.state).0),
                        ));
                        let ids: Vec<_> = r.run.agents.iter().map(|a| a.id.clone()).collect();
                        lives = lives.push(kv("agents", ids.join(", ")));
                        let mut branches: Vec<String> = vec![];
                        for a in d.agents.iter().filter(|a| ids.contains(&a.id)) {
                            let b = if a.branch.is_empty() {
                                "main".to_owned()
                            } else {
                                a.branch.clone()
                            };
                            if !branches.contains(&b) {
                                branches.push(b);
                            }
                        }
                        lives = lives.push(kv("branches", branches.join(", ")));
                    } else if !t.status.branch.is_empty() {
                        lives = lives.push(kv("branches", t.status.branch.clone()));
                    } else {
                        lives = lives.push(
                            text("Not started: no run, agent or branch yet.")
                                .size(12)
                                .color(DIM),
                        );
                    }
                    col = col.push(lives);
                    col.into()
                }
            };
        let detail = match (&self.task_form, &self.importing) {
            (Some(form), _) => self.task_form_view(form),
            (None, Some(issue)) => self.import_view(issue),
            (None, None) => detail,
        };
        row![
            board,
            container(text(""))
                .width(1)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(LINE)),
                    ..container::Style::default()
                }),
            container(self.detail(detail))
                .width(430)
                .height(Length::Fill)
                .style(theme::inspector_style),
        ]
        .height(Length::Fill)
        .into()
    }

    /// The one-field form that imports a GitHub issue as a task.
    fn import_view<'a>(&'a self, issue: &'a str) -> Element<'a, Message> {
        container(
            column![
                text("Import a GitHub issue").size(18).color(theme::HI),
                theme::label("ISSUE NUMBER, OWNER/REPO#N OR URL"),
                text_input("7", issue)
                    .on_input(|t| Message::Panel(PanelMsg::ImportInput(t)))
                    .on_submit(Message::Panel(PanelMsg::ImportSubmit))
                    .style(theme::input_style)
                    .padding([8, 10])
                    .size(13),
                text("Needs the GitHub CLI (`gh`) signed in. The title and body become the task.")
                    .size(11)
                    .color(DIM),
                row![
                    btn_primary("Import", PanelMsg::ImportSubmit),
                    btn("Cancel", PanelMsg::ImportCancel),
                ]
                .spacing(10),
            ]
            .spacing(10),
        )
        .max_width(600)
        .into()
    }

    /// The form that creates a task or edits one: title, description, criteria (a line each) and, when editing,
    /// the spec.
    fn task_form_view<'a>(&'a self, form: &'a TaskForm) -> Element<'a, Message> {
        let editor = |content: &'a text_editor::Content,
                      action: fn(text_editor::Action) -> PanelMsg,
                      height: f32| {
            text_editor(content)
                .on_action(move |a| Message::Panel(action(a)))
                .height(Length::Fixed(height))
                .padding(10)
                .size(13)
                .style(theme::editor_style)
        };
        let mut col = column![
            row![
                text(match form.editing {
                    Some(id) => format!("Edit task #{id}"),
                    None => "New task".to_owned(),
                })
                .size(18)
                .color(theme::HI),
                container(text("")).width(Length::Fill),
                btn_primary("Save", PanelMsg::TaskSave),
                btn("Cancel", PanelMsg::TaskCancel),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            column![
                theme::label("TITLE"),
                text_input("What has to be done", &form.title)
                    .on_input(|t| Message::Panel(PanelMsg::TaskTitle(t)))
                    .on_submit(Message::Panel(PanelMsg::TaskSave))
                    .style(theme::input_style_marked(
                        crate::ui::is_invalid(&self.notice) && form.title.trim().is_empty(),
                    ))
                    .padding([8, 10])
                    .size(13),
            ]
            .spacing(6),
            column![
                theme::label("DESCRIPTION"),
                editor(&form.description, PanelMsg::TaskDescription, 110.0),
            ]
            .spacing(6),
            column![
                theme::label("ACCEPTANCE CRITERIA · ONE PER LINE"),
                editor(&form.criteria, PanelMsg::TaskCriteria, 130.0),
            ]
            .spacing(6),
        ]
        .spacing(14);
        if form.editing.is_some() {
            col = col.push(
                column![
                    theme::label("SPEC"),
                    editor(&form.spec, PanelMsg::TaskSpec, 160.0),
                ]
                .spacing(6),
            );
        }
        container(col).max_width(900).into()
    }

    fn patterns_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        use shepherd_app_core::shepherd_core::patterns::PatternSource;
        let entry = |p: &'a shepherd_app_core::shepherd_core::patterns::PatternSummary| -> Element<'a, Message> {
            let body = column![
                text(p.name.clone()).font(theme::MONO_BOLD).size(13).color(theme::HI),
                text(format!("{:?}", p.kind).to_lowercase()).size(11).color(DIM),
                text(format!("{} agents, {} topics", p.agents_count, p.topics_count)).size(11).color(DIM),
            ]
            .spacing(3);
            button(body)
                .width(Length::Fill)
                .padding([8, 10])
                .style(item_style(self.pattern_sel.as_deref() == Some(p.name.as_str())))
                .on_press(crate::ui::nav(
                    self.pattern_sel.as_deref() == Some(p.name.as_str()),
                    Message::Panel(PanelMsg::SelectPattern(p.name.clone())),
                ))
                .into()
        };
        let heading = |s: &'static str| {
            container(text(s).size(11.5).color(DIM)).padding(pad(10.0, 4.0, 4.0, 4.0))
        };
        let (mine, builtin): (Vec<_>, Vec<_>) = ctx
            .data
            .patterns
            .iter()
            .filter(|p| self.passes(&format!("{} {:?}", p.name, p.kind)))
            .partition(|p| !matches!(p.source, PatternSource::Builtin));
        let mut items: Vec<Element<'a, Message>> = vec![];
        if !mine.is_empty() {
            items.push(heading("Yours in this project").into());
            items.extend(mine.into_iter().map(entry));
        }
        if !builtin.is_empty() {
            items.push(heading("Built in").into());
            items.extend(builtin.into_iter().map(entry));
        }
        let top = crate::ui::filter_bar("Filter patterns", &self.filter, &[], 0);
        let detail: Element<'a, Message> = match &self.edit {
            None => {
                let message = if ctx.data.patterns.is_empty() {
                    "No patterns in this project. Create one from the configuration or add a pattern file."
                } else if self.pattern_sel.is_some() {
                    "Loading pattern…"
                } else {
                    "Pick a pattern to inspect and edit its graph."
                };
                container(
                    column![
                        text(if ctx.data.patterns.is_empty() {
                            "No patterns yet"
                        } else {
                            "Select a pattern"
                        })
                        .size(16),
                        text(message).size(12).color(DIM),
                    ]
                    .spacing(8),
                )
                .padding(18)
                .style(card_style)
                .into()
            }
            Some(edit) => column![edit.view(
                ctx.frame,
                self.armed == Some(Armed::DeletePattern(edit.editor.pattern.name.clone()))
            ),]
            .spacing(10)
            .into(),
        };
        row![
            list_box_sized(260.0, Some(top), items, vec![]),
            container(detail)
                .padding([16, 20])
                .width(Length::Fill)
                .height(Length::Fill),
        ]
        .height(Length::Fill)
        .into()
    }

    /// Every notice of this session, oldest first (the status bar shows them in a list).
    pub fn history(&self) -> &[(String, String)] {
        &self.history
    }

    /// Forgets the notices kept.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Esc: clears the filter box of the current list.
    pub fn clear_filter(&mut self) {
        self.filter.clear();
    }

    /// Esc: an armed button goes back to normal.
    pub fn disarm(&mut self) {
        self.armed = None;
        self.armed_at = None;
    }

    /// An armed button goes back to normal after `ARM_SECS` (called on every tick).
    pub fn expire_armed(&mut self) {
        self.toasts.retain(|t| !t.expired());
        if self.armed.is_some()
            && self
                .armed_at
                .is_none_or(|t| t.elapsed().as_secs_f32() >= ARM_SECS)
        {
            self.armed = None;
            self.armed_at = None;
        }
    }

    /// Whether the communication screen of a run is open (it reads the bus again on every change).
    pub fn comm_open(&self) -> bool {
        self.comm_open
    }

    /// The least width the main area keeps: the fixed panes of the view plus a flexible region of 360 px.
    /// Narrower than this, the main area scrolls sideways instead of squashing anything.
    pub fn min_main_width(&self) -> f32 {
        const FLEX: f32 = 360.0;
        match self.tab {
            Tab::Overview | Tab::Git | Tab::Monitor => 1000.0,
            Tab::Runs | Tab::Skills => 320.0 + 1.0 + FLEX,
            Tab::Agents => 460.0 + 1.0 + FLEX,
            Tab::Tasks => 430.0 + 1.0 + FLEX,
            Tab::Patterns => 260.0 + 300.0 + 2.0 + FLEX,
            Tab::Events => 250.0 + 1.0 + FLEX,
            Tab::Models => 230.0 + 1.0 + FLEX,
            Tab::Snapshots => 400.0 + 1.0 + FLEX,
        }
    }

    /// The seconds the Activity window covers.
    fn activity_secs(&self) -> u64 {
        [300, 1800, 7200][self.activity_window.min(2)]
    }

    /// Activity: what every agent did over the window, and how far each reader is behind on the bus.
    fn monitor_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        // one flexible region: the page keeps at least 900 px and scrolls both ways when narrower
        let (this, ctx) = (self, ctx.clone());
        crate::ui::flex(900.0, [20.0, 28.0], move || this.activity_page(&ctx))
    }

    fn activity_page<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        use shepherd_app_core::timeline;
        let d = ctx.data;
        let secs = self.activity_secs();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |t| t.as_secs());
        let from = now.saturating_sub(secs);
        let state_color = |s: &str| match s {
            "working" | "starting" => ACCENT,
            "blocked" => WARN,
            "done" => OK,
            "failed" | "killed" => BAD,
            timeline::UNKNOWN => theme::LINE,
            _ => theme::LINE2,
        };
        let legend = |c: Color, n: &'static str| {
            row![text("■").size(10).color(c), text(n).size(11).color(DIM)].spacing(4)
        };
        let mut rows = column![].spacing(14);
        if d.agents.is_empty() {
            rows = rows.push(
                text("No agents yet. Their timelines appear here once they start.")
                    .size(12)
                    .color(DIM),
            );
        }
        for a in &d.agents {
            let segs = timeline::segments(&d.state_log, &a.id, &a.state, from, now);
            let mut bar = row![].spacing(0).height(10);
            for (state, dur) in &segs {
                let c = state_color(state);
                bar = bar.push(
                    container(text(""))
                        .width(Length::FillPortion((*dur).clamp(1, 65_000) as u16))
                        .height(10)
                        .style(move |_| container::Style {
                            background: Some(Background::Color(c)),
                            ..container::Style::default()
                        }),
                );
            }
            let status = agent_status(&a.state);
            let word = if a.state == "blocked" {
                "needs you"
            } else {
                a.state.as_str()
            };
            // columns: 170 (state, id, word) | lane (flexible, 400 at least) | 250 (current task); the row opens the agent
            rows = rows.push(
                button(
                    row![
                        row![
                            text("■").size(10).color(status_color(status)),
                            text(a.id.clone())
                                .font(theme::MONO_BOLD)
                                .size(12.5)
                                .color(theme::HI),
                            text(word).size(11).color(status_color(status)),
                        ]
                        .spacing(8)
                        .width(170),
                        container(bar)
                            .padding([2, 2])
                            .width(Length::Fill)
                            .style(|_| container::Style {
                                background: Some(Background::Color(Color::from_rgb8(8, 8, 11))),
                                border: iced::Border {
                                    color: Color::from_rgb8(0x15, 0x15, 0x1C),
                                    width: 1.0,
                                    radius: 3.0.into(),
                                },
                                ..container::Style::default()
                            }),
                        container(
                            text(if a.task_text.is_empty() {
                                String::new()
                            } else {
                                a.task_text.chars().take(32).collect()
                            })
                            .size(11.5)
                            .color(DIM)
                        )
                        .width(250)
                        .clip(true),
                    ]
                    .spacing(16)
                    .align_y(iced::Alignment::Center),
                )
                .width(Length::Fill)
                .height(38)
                .padding([0, 4])
                .style(item_style(false))
                .on_press(Message::Panel(PanelMsg::SelectAgent(a.id.clone()))),
            );
        }
        // time axis: five marks from the start of the window to now
        let mut axis = row![container(text("")).width(170)].spacing(16);
        let mut marks = row![].width(Length::Fill);
        for k in 0..5u64 {
            let t = from + secs * k / 4;
            let label = if k == 4 { "now".to_owned() } else { clock(t) };
            marks = marks.push(text(label).size(10.5).color(DIM).width(Length::Fill));
        }
        axis = axis.push(marks).push(container(text("")).width(250));
        let timelines = card(
            "AGENT TIMELINES",
            Some(
                row![
                    legend(ACCENT, "working"),
                    legend(theme::LINE2, "idle"),
                    legend(WARN, "blocked"),
                    legend(OK, "done")
                ]
                .spacing(12)
                .into(),
            ),
            column![rows, axis].spacing(14).into(),
        );

        // reader lag: per topic and consumer group
        let mut lag_rows = column![].spacing(14);
        let topics = self.cfg.topic_rows();
        let max_lag = topics
            .iter()
            .flat_map(|t| t.groups.iter())
            .map(|g| g.lag_by_partition.values().sum::<u64>())
            .max()
            .unwrap_or(0)
            .max(1);
        let mut any = false;
        for t in topics {
            for (k, g) in t.groups.iter().enumerate() {
                any = true;
                let lag: u64 = g.lag_by_partition.values().sum();
                let mut parts: Vec<(u32, u64)> =
                    g.lag_by_partition.iter().map(|(p, l)| (*p, *l)).collect();
                parts.sort_unstable();
                let detail = if parts.is_empty() {
                    "p0 0".to_owned()
                } else {
                    parts
                        .iter()
                        .map(|(p, l)| format!("p{p} {l}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let reach = 0.08 + 0.9 * lag as f64 / max_lag as f64;
                // columns: 120 (#topic) | 220 (group, members) | lag bar (flexible) | 150 (lag, per partition);
                // the row opens the topic in Events
                lag_rows = lag_rows.push(
                    button(
                        row![
                            text(format!("#{}", t.name))
                                .size(12.5)
                                .color(theme::VIOLET)
                                .width(120),
                            container(
                                row![
                                    text(g.group.clone())
                                        .font(theme::MONO_BOLD)
                                        .size(12.5)
                                        .color(theme::HI),
                                    text(g.members.join(", ")).size(11).color(DIM),
                                ]
                                .spacing(8)
                            )
                            .width(220)
                            .clip(true),
                            Canvas::new(Prism {
                                preset: Preset::lag(reach, (k as f64) * 7.0),
                                frame: ctx.frame.wrapping_add(k as u32 * 3),
                            })
                            .width(Length::Fill)
                            .height(16),
                            container(
                                row![
                                    if lag == 0 {
                                        text("caught up").size(12).color(OK)
                                    } else {
                                        text(format!("lag {lag}")).size(12).color(if lag > 1 {
                                            WARN
                                        } else {
                                            FG
                                        })
                                    },
                                    text(detail).size(11).color(theme::FAINT),
                                ]
                                .spacing(8)
                            )
                            .width(150)
                            .clip(true),
                        ]
                        .spacing(16)
                        .align_y(iced::Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding([6, 4])
                    .style(item_style(false))
                    .on_press(Message::Panel(PanelMsg::OpenTopic(t.name.clone()))),
                );
            }
        }
        if !any {
            lag_rows = lag_rows.push(
                text("No reader has joined a topic yet.")
                    .size(12)
                    .color(DIM),
            );
        }
        let lag = card(
            "READER LAG",
            Some(btn_link("Open events", PanelMsg::Tab(Tab::Events))),
            column![
                text("Messages each consumer group has not read yet, per topic. A longer prism means more waiting.")
                    .size(11.5)
                    .color(DIM),
                lag_rows
            ]
            .spacing(12)
            .into(),
        );
        crate::theme::scroll(
            container(column![timelines, lag].spacing(16))
                .padding([20, 28])
                .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    /// The communication of a run: who said what to whom, filtered by sender and topic, with a box to write to
    /// the team as the human.
    fn comm_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let d = ctx.data;
        let run = self
            .monitor
            .as_deref()
            .and_then(|id| d.runs.iter().find(|r| r.run.id == id));
        let topics: Vec<String> = run.map(|r| r.run.topics.clone()).unwrap_or_default();
        let chip = |label: String, on: bool, t: Option<String>| {
            button(text(label).size(11.5))
                .padding([3, 10])
                .style(theme::chip_style(on))
                .on_press(Message::Panel(PanelMsg::CommTopic(t)))
        };
        let mut chips = row![chip("All topics".into(), self.comm_topic.is_none(), None)].spacing(6);
        for t in &topics {
            chips = chips.push(chip(
                format!("#{t}"),
                self.comm_topic.as_deref() == Some(t.as_str()),
                Some(t.clone()),
            ));
        }
        let title = column![
            row![
                button(text("Runs").size(11.5).color(ACCENT))
                    .padding(0)
                    .style(theme::link_button_style())
                    .on_press(Message::Panel(PanelMsg::CommClose)),
                text("›").size(11.5).color(DIM),
                text(
                    run.map(|r| r.run.id.clone())
                        .unwrap_or_else(|| "all agents".into())
                )
                .size(11.5)
                .color(DIM),
            ]
            .spacing(6),
            text("What the agents say to each other")
                .font(theme::MONO_BOLD)
                .size(20)
                .color(theme::HI),
        ]
        .spacing(6);
        let header = container(
            row![
                title,
                container(text("")).width(Length::Fill),
                chips.wrap(),
                btn("Back to the run", PanelMsg::CommClose),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
        )
        .padding([14, 28])
        .width(Length::Fill)
        .style(theme::bottom_line_style);

        // senders: the run's agents and the human
        let mut senders = column![theme::label("SHOW MESSAGES FROM")].spacing(10);
        let agents: Vec<String> = match run {
            Some(r) => r.run.agents.iter().map(|a| a.id.clone()).collect(),
            None => d.agents.iter().map(|a| a.id.clone()).collect(),
        };
        let mut names: Vec<(String, String, Color)> = agents
            .iter()
            .map(|id| {
                let st = d
                    .agents
                    .iter()
                    .find(|a| &a.id == id)
                    .map_or("", |a| a.state.as_str());
                let word = if st == "blocked" { "needs you" } else { st };
                (id.clone(), word.to_owned(), status_color(agent_status(st)))
            })
            .collect();
        names.push(("user".into(), "human".into(), theme::VIOLET));
        for (id, word, color) in names {
            let shown = !self.comm_hidden.contains(&id);
            let key = id.clone();
            senders = senders.push(
                row![
                    checkbox(shown)
                        .on_toggle(move |_| Message::Panel(PanelMsg::CommToggle(key.clone())))
                        .size(14),
                    text("■").size(9).color(color),
                    text(id.clone()).size(12.5).width(Length::Fill),
                    text(word).size(11).color(color),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }
        let side = container(senders)
            .padding(16)
            .width(240)
            .height(Length::Fill);

        // messages
        let mut feed = column![].spacing(0);
        let visible: Vec<&BusMessage> = self
            .feed
            .iter()
            .filter(|m| !self.comm_hidden.contains(&m.from))
            .filter(|m| self.comm_topic.as_deref().is_none_or(|t| t == m.topic))
            .collect();
        if visible.is_empty() {
            feed = feed.push(
                column![
                    text("No communication yet").size(14).color(theme::HI),
                    text("Messages appear here as the agents publish to this run's topics.")
                        .size(12)
                        .color(DIM),
                ]
                .spacing(6),
            );
        }
        for m in visible {
            let time: String = m.ts.chars().skip(11).take(8).collect();
            let who_color = theme::emitter_color(&m.from);
            let system = m.from.is_empty() || m.kind == "event" || m.kind == "system";
            if system {
                feed = feed.push(
                    container(row![
                        text(time).size(11.5).color(DIM).width(80),
                        text(m.content.clone()).size(12).color(DIM),
                    ])
                    .padding([6, 0]),
                );
                continue;
            }
            let letter: String = m
                .from
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default();
            let mut head = row![text(m.from.clone())
                .font(theme::MONO_BOLD)
                .size(12.5)
                .color(theme::HI)]
            .spacing(8)
            .align_y(iced::Alignment::Center);
            if !m.to.is_empty() {
                head = head
                    .push(text("to").size(11.5).color(DIM))
                    .push(text(m.to.clone()).size(12).color(FG));
            }
            if !m.topic.is_empty() {
                head = head.push(text("to").size(11.5).color(DIM)).push(
                    container(
                        text(format!("#{} p{}", m.topic, m.partition))
                            .size(11)
                            .color(theme::VIOLET),
                    )
                    .padding([0, 6])
                    .style(|_| container::Style {
                        border: iced::Border {
                            color: theme::VIOLET,
                            width: 1.0,
                            radius: 3.0.into(),
                        },
                        ..container::Style::default()
                    }),
                );
            }
            feed = feed.push(
                container(
                    row![
                        text(time).size(11.5).color(DIM).width(80),
                        container(
                            text(letter)
                                .size(11)
                                .font(theme::MONO_BOLD)
                                .color(theme::BG)
                        )
                        .width(22)
                        .height(22)
                        .center_x(22)
                        .center_y(22)
                        .style(move |_| container::Style {
                            background: Some(Background::Color(who_color)),
                            border: iced::Border {
                                radius: 4.0.into(),
                                ..iced::Border::default()
                            },
                            ..container::Style::default()
                        }),
                        column![head, text(m.content.clone()).size(13).color(theme::FG)].spacing(3),
                    ]
                    .spacing(12),
                )
                .padding([9, 0]),
            );
        }
        // a gate that waits for the human closes the feed
        let task_id = run.and_then(|r| task_of_run(&r.run.task));
        if let Some(g) = d.gates.iter().find(|g| Some(g.task_id) == task_id) {
            feed = feed.push(alert(
                row![
                    text(format!(
                        "{} waits for a human. Nothing is pushed until you approve.",
                        g.gate.label()
                    ))
                    .size(12.5)
                    .width(Length::Fill),
                    btn("Decide in the run", PanelMsg::CommClose),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center)
                .into(),
                WARN,
            ));
        }
        let target = self.comm_topic.clone().or_else(|| topics.first().cloned());
        let composer = container(
            row![
                text(match &target {
                    Some(t) => format!("to #{t}"),
                    None => "no topic".into(),
                })
                .size(12)
                .color(DIM),
                text_input("Tell the team something, sent as you", &self.comm_text)
                    .on_input(|t| Message::Panel(PanelMsg::CommText(t)))
                    .on_submit(Message::Panel(PanelMsg::CommSend))
                    .style(theme::input_style)
                    .padding([9, 12])
                    .size(12.5)
                    .width(Length::Fill),
                btn_primary("Send", PanelMsg::CommSend),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
        )
        .padding([0, 28])
        .width(Length::Fill)
        .height(64)
        .align_y(iced::alignment::Vertical::Center)
        .style(theme::top_line_style);
        column![
            header,
            row![
                side,
                container(text(""))
                    .width(1)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(LINE)),
                        ..container::Style::default()
                    }),
                crate::theme::scroll(container(feed).padding([8, 28]).width(Length::Fill))
                    .height(Length::Fill),
            ]
            .height(Length::Fill),
            composer,
        ]
        .into()
    }
}

/// `HH:MM` (UTC) of a unix time, for the Activity axis.
fn clock(t: u64) -> String {
    format!("{:02}:{:02}", (t / 3600) % 24, (t / 60) % 60)
}

/// `.list`: 300 px column with a 1 px line on its right, items on top and the footer buttons below.
pub fn list_box<'a>(
    items: Vec<Element<'a, Message>>,
    footer: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    list_box_with(None, items, footer)
}

/// `list_box` with something (the filter) above the items.
pub fn list_box_with<'a>(
    top: Option<Element<'a, Message>>,
    items: Vec<Element<'a, Message>>,
    footer: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    list_box_sized(320.0, top, items, footer)
}

/// A fixed list pane `width` wide (320 for Runs and Skills, 250 for Events, 260 for Patterns, ...).
pub fn list_box_sized<'a>(
    width: f32,
    top: Option<Element<'a, Message>>,
    items: Vec<Element<'a, Message>>,
    footer: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let line = || {
        container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(|_| container::Style {
                background: Some(Background::Color(LINE)),
                ..container::Style::default()
            })
    };
    let list = crate::theme::scroll(column(items).spacing(2).padding(8)).height(Length::Fill);
    let mut col = column![];
    if let Some(top) = top {
        col = col.push(top);
    }
    col = col.push(list);
    if !footer.is_empty() {
        col = col
            .push(line())
            .push(container(row(footer).spacing(4).wrap()).padding(8));
    }
    row![
        container(col).width(width).height(Length::Fill),
        container(text(""))
            .width(1)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(LINE)),
                ..container::Style::default()
            }),
    ]
    .height(Length::Fill)
    .into()
}

/// `.detail`: the rest of the width, padded, no card.
pub fn detail_box<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    container(crate::theme::scroll(
        container(content).padding([20, 24]).width(Length::Fill),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepherd_app_core::data::TaskRow;
    use shepherd_app_core::shepherd_core::TaskStatus;

    fn task(id: u32) -> TaskRow {
        TaskRow {
            status: TaskStatus {
                id,
                slug: "t".into(),
                title: "Login".into(),
                state: "new".into(),
                issue: None,
                branch: String::new(),
                pr: 0,
                subtasks: vec![],
                waiting: String::new(),
                note: String::new(),
                agents: vec![],
                created: String::new(),
                updated: String::new(),
            },
            description: "Para entrar".into(),
            criteria: vec!["- [ ] funciona".into()],
            issue_body: "Para entrar".into(),
            spec: String::new(),
        }
    }

    #[test]
    fn the_launcher_for_a_task_starts_from_its_text_and_stop_needs_two_clicks() {
        let data = Data {
            tasks: vec![task(3)],
            ..Data::default()
        };
        let snapshot = Snapshot::default();
        let ctx = Ctx {
            api: PathBuf::from("/nowhere"),
            data: &data,
            snapshot: &snapshot,
            frame: 0,
        };
        let mut panel = Panel::default();
        let _ = panel.update(PanelMsg::OpenLauncher(Some(3)), &ctx);
        assert!(panel.launching);
        assert_eq!(panel.tab, Tab::Runs);
        assert!(panel
            .objective
            .text()
            .starts_with("Task #3: Login\n\nPara entrar"));

        // the first click on Stop only arms it; anything else disarms it
        let _ = panel.update(PanelMsg::Control("run-1".into(), RunControl::Stop), &ctx);
        assert_eq!(panel.armed, Some(Armed::StopRun("run-1".into())));
        let _ = panel.update(PanelMsg::Tab(Tab::Agents), &ctx);
        assert_eq!(panel.armed, None);
    }

    fn snap(id: &str, created: u64) -> shepherd_app_core::shepherd_core::snapshot::SnapshotInfo {
        use shepherd_app_core::shepherd_core::snapshot::{SnapshotInfo, SnapshotReason};
        let (agent, n) = id.split_once('/').unwrap();
        SnapshotInfo {
            id: id.into(),
            agent: agent.into(),
            n: n.parse().unwrap(),
            commit: "abcdef0123".into(),
            reason: SnapshotReason::Manual,
            created,
            worktree: "/w".into(),
            prev: None,
            files: 1,
            added: 2,
            removed: 0,
            omitted: vec![],
        }
    }

    #[test]
    fn the_snapshots_tab_shows_the_picked_snapshot_else_the_newest_and_changes_need_two_clicks() {
        let data = Data {
            snapshots: vec![snap("coder-1/2", 200), snap("coder-1/1", 100)],
            ..Data::default()
        };
        let snapshot = Snapshot::default();
        let ctx = Ctx {
            api: PathBuf::from("/nowhere"),
            data: &data,
            snapshot: &snapshot,
            frame: 0,
        };
        let mut panel = Panel::default();
        assert_eq!(panel.snap_id(&ctx).as_deref(), Some("coder-1/2"));
        let _ = panel.update(PanelMsg::SnapSelect("coder-1/1".into()), &ctx);
        assert_eq!(panel.snap_id(&ctx).as_deref(), Some("coder-1/1"));
        // a picked snapshot that was deleted falls back to the newest
        panel.snap = Some("gone/9".into());
        assert_eq!(panel.snap_id(&ctx).as_deref(), Some("coder-1/2"));
        // the detail of another snapshot is not shown for this one
        let _ = panel.update(
            PanelMsg::SnapShown("coder-1/1".into(), Err("late answer".into())),
            &ctx,
        );
        assert!(panel.snap_detail.is_none());

        let restore = Armed::RestoreSnapshot("coder-1/2".into());
        let _ = panel.update(PanelMsg::Arm(restore.clone()), &ctx);
        assert_eq!(panel.armed, Some(restore));
        let _ = panel.update(PanelMsg::Tab(Tab::Snapshots), &ctx);
        assert_eq!(panel.armed, None, "anything else disarms it");

        // pruning also asks twice; a comparison belongs to the snapshot it was asked for
        let prune = Armed::PruneSnapshots("coder-1".into());
        let _ = panel.update(PanelMsg::Arm(prune.clone()), &ctx);
        assert_eq!(panel.armed, Some(prune));
        panel.snap_compare = Some(("coder-1/2".into(), Err("x".into())));
        let _ = panel.update(PanelMsg::SnapSelect("coder-1/1".into()), &ctx);
        assert!(panel.snap_compare.is_none());
        let _ = panel.update(
            PanelMsg::SnapCompared("coder-1/2".into(), Err("late answer".into())),
            &ctx,
        );
        assert!(
            panel.snap_compare.is_none(),
            "another snapshot's answer is dropped"
        );
        assert!(Panel::default()
            .snap_id(&Ctx {
                data: &Data::default(),
                ..ctx
            })
            .is_none());
    }

    #[test]
    fn an_agents_skills_can_be_changed_one_row_each_with_a_note() {
        use shepherd_app_core::shepherd_core::{Agent, SkillAdjust};
        assert!(agent_skill_buttons(&Agent::default()).is_empty());
        let agent = Agent {
            id: "coder-1".into(),
            active_skills: vec!["x".into(), "y".into()],
            skills: SkillAdjust {
                add: vec!["z".into()],
                remove: vec!["y".into()],
            },
            ..Agent::default()
        };
        // x (active), y (removed, can come back), z (added, not installed yet), and the note
        assert_eq!(agent_skill_buttons(&agent).len(), 4);
    }

    #[test]
    fn an_agents_skills_card_lists_what_is_active_and_what_was_changed_for_it() {
        use shepherd_app_core::shepherd_core::{Agent, SkillAdjust};
        let none = Agent::default();
        assert_eq!(
            agent_skill_lines(&none),
            ["No skills were installed for this agent."]
        );
        let agent = Agent {
            active_skills: vec!["shepherd-comms".into(), "debug".into()],
            skills: SkillAdjust {
                add: vec!["debug".into()],
                remove: vec!["review".into()],
            },
            ..Agent::default()
        };
        assert_eq!(
            agent_skill_lines(&agent),
            [
                "Active: shepherd-comms, debug",
                "Added for this agent: debug",
                "Removed for this agent: review"
            ]
        );
    }

    #[test]
    fn a_snapshot_is_taken_in_the_agents_worktree_and_restored_where_it_was_taken() {
        use shepherd_app_core::shepherd_core::Agent;
        let agent = |id: &str, worktree: &str, root: &str| Agent {
            id: id.into(),
            worktree: worktree.into(),
            root: root.into(),
            ..Agent::default()
        };
        let mut shot = snap("coder-1/1", 100);
        shot.worktree = "/wt/coder-1".into();
        let data = Data {
            root: PathBuf::from("/proj"),
            agents: vec![
                agent("coder-1", "/wt/coder-1", "/proj"),
                agent("docs-1", "", "/other"),
            ],
            snapshots: vec![shot, snap("old/1", 50)],
            ..Data::default()
        };
        assert_eq!(snapshots::capture_root(&data, "coder-1"), "/wt/coder-1");
        // an agent without a worktree is snapshotted in its project, an unknown one in ours
        assert_eq!(snapshots::capture_root(&data, "docs-1"), "/other");
        assert_eq!(snapshots::capture_root(&data, "ghost"), "/proj");
        assert_eq!(snapshots::restore_root(&data, "coder-1/1"), "/wt/coder-1");
        assert_eq!(snapshots::restore_root(&data, "old/1"), "/w");
        assert_eq!(snapshots::restore_root(&data, "missing/1"), "/proj");
    }

    #[test]
    fn approving_or_rejecting_a_gate_needs_two_clicks_and_clears_the_arming() {
        let data = Data::default();
        let snapshot = Snapshot::default();
        let ctx = Ctx {
            api: PathBuf::from("/nowhere"),
            data: &data,
            snapshot: &snapshot,
            frame: 0,
        };
        let mut panel = Panel::default();
        let approve = Armed::ApproveGate(4, Gate::Plan);
        let _ = panel.update(PanelMsg::Arm(approve.clone()), &ctx);
        assert_eq!(panel.armed, Some(approve));
        // arming the other button replaces the first one
        let reject = Armed::RejectGate(4, Gate::PushPr(2));
        let _ = panel.update(PanelMsg::Arm(reject.clone()), &ctx);
        assert_eq!(panel.armed, Some(reject));
        // the second click sends the request and leaves nothing armed
        let _ = panel.update(PanelMsg::Confirm, &ctx);
        assert_eq!(panel.armed, None);
    }

    fn ctx_of<'a>(data: &'a Data, snapshot: &'a Snapshot) -> Ctx<'a> {
        Ctx {
            api: PathBuf::from("/nowhere"),
            data,
            snapshot,
            frame: 0,
        }
    }

    #[test]
    fn cancelling_a_run_needs_two_clicks_and_only_a_started_task_has_one() {
        let (data, snapshot) = (Data::default(), Snapshot::default());
        let ctx = ctx_of(&data, &snapshot);
        let mut panel = Panel::default();
        let _ = panel.update(PanelMsg::Arm(Armed::CancelRun(3)), &ctx);
        assert_eq!(panel.armed, Some(Armed::CancelRun(3)));
        let _ = panel.update(PanelMsg::Confirm, &ctx);
        assert_eq!(panel.armed, None);
        for state in ["coding", "planning", "waiting", "awaiting-approval"] {
            assert!(has_run(state), "{state}");
        }
        for state in ["new", "done", "completed", "archived", "cancelled"] {
            assert!(!has_run(state), "{state}");
        }
    }

    #[test]
    fn the_import_form_opens_takes_text_and_closes() {
        let (data, snapshot) = (Data::default(), Snapshot::default());
        let ctx = ctx_of(&data, &snapshot);
        let mut panel = Panel::default();
        // typing without the form open does nothing
        let _ = panel.update(PanelMsg::ImportInput("7".into()), &ctx);
        assert_eq!(panel.importing, None);

        let _ = panel.update(PanelMsg::ImportOpen, &ctx);
        let _ = panel.update(PanelMsg::ImportInput("#7".into()), &ctx);
        assert_eq!(panel.importing.as_deref(), Some("#7"));
        // an empty reference is not sent: the form stays and says why
        let _ = panel.update(PanelMsg::ImportInput("  ".into()), &ctx);
        let _ = panel.update(PanelMsg::ImportSubmit, &ctx);
        assert_eq!(panel.importing.as_deref(), Some("  "));
        assert!(panel.notice.contains("issue number"));
        // a reference is sent and closes the form
        let _ = panel.update(PanelMsg::ImportInput("7".into()), &ctx);
        let _ = panel.update(PanelMsg::ImportSubmit, &ctx);
        assert_eq!(panel.importing, None);
        // opening the task form or cancelling closes it too
        let _ = panel.update(PanelMsg::ImportOpen, &ctx);
        let _ = panel.update(PanelMsg::TaskNew, &ctx);
        assert_eq!(panel.importing, None);
        let _ = panel.update(PanelMsg::ImportOpen, &ctx);
        assert!(
            panel.task_form.is_none(),
            "the import form replaces the task form"
        );
        let _ = panel.update(PanelMsg::ImportCancel, &ctx);
        assert_eq!(panel.importing, None);
    }

    #[test]
    fn a_rejection_carries_the_reason_once_and_a_blank_one_is_not_sent() {
        assert_eq!(reason_of("  too broad \n").as_deref(), Some("too broad"));
        assert_eq!(reason_of("   "), None);
        assert_eq!(reason_of(""), None);

        let (data, snapshot) = (Data::default(), Snapshot::default());
        let ctx = ctx_of(&data, &snapshot);
        let mut panel = Panel::default();
        let _ = panel.update(PanelMsg::RejectReason("split it".into()), &ctx);
        assert_eq!(panel.reject_reason, "split it");
        // arming does not use it up; confirming does, so the next rejection starts empty
        let _ = panel.update(PanelMsg::Arm(Armed::RejectGate(4, Gate::Plan)), &ctx);
        assert_eq!(panel.reject_reason, "split it");
        let _ = panel.update(PanelMsg::Confirm, &ctx);
        assert_eq!(panel.reject_reason, "");
    }

    #[test]
    fn a_new_notice_becomes_a_toast_a_refusal_stays_and_the_filter_needs_every_word() {
        let mut panel = Panel {
            notice: "Saved task #1".into(),
            ..Panel::default()
        };
        panel.sync_toast();
        assert!(panel.toasts().last().is_some_and(|(_, t)| !t.error));
        panel.notice = "error: refused: cannot remove".into();
        panel.sync_toast();
        let toast = panel
            .toasts()
            .last()
            .map(|(_, t)| *t)
            .expect("a refusal stays");
        assert!(toast.error && !toast.expired());
        for n in 0..5 {
            panel.notice = format!("Saved {n}");
            panel.sync_toast();
        }
        assert_eq!(panel.toasts().len(), 3, "at most three at a time");
        assert_eq!(panel.history.len(), 7, "every notice is kept");
        panel.update_filter_for_test("api team");
        assert!(!panel.passes("api only 231"));
        assert!(panel.passes("api team 231"));
        assert!(panel.passes("TEAM api"));
    }

    #[test]
    fn the_snapshot_filters_pick_an_agent_and_a_reason_and_the_view_builds_with_them() {
        let data = Data {
            snapshots: vec![
                snap("coder-1/2", 200),
                snap("coder-2/1", 150),
                snap("coder-1/1", 100),
            ],
            ..Data::default()
        };
        let snapshot = Snapshot::default();
        let ctx = ctx_of(&data, &snapshot);
        let mut panel = Panel {
            tab: Tab::Snapshots,
            ..Panel::default()
        };
        // with no agent alive the Capture box falls back to the shown snapshot's agent
        assert_eq!(panel.capture_agent(&ctx).as_deref(), Some("coder-1"));
        let _ = panel.update(PanelMsg::SnapFilterAgent(Some("coder-2".into())), &ctx);
        let _ = panel.update(PanelMsg::SnapFilterReason(Some("idle".into())), &ctx);
        assert_eq!(panel.snap_agent_filter.as_deref(), Some("coder-2"));
        let _ = panel.snapshots_view(&ctx);
        let _ = panel.header(&ctx);
        let _ = panel.update(PanelMsg::SnapFilterAgent(None), &ctx);
        assert!(panel.snap_agent_filter.is_none());
    }

    #[test]
    fn a_new_pattern_opens_the_editor_on_something_to_save_and_deleting_needs_two_clicks() {
        let data = Data::default();
        let snapshot = Snapshot::default();
        let ctx = ctx_of(&data, &snapshot);
        let mut panel = Panel::default();
        let _ = panel.update(PanelMsg::NewPattern, &ctx);
        let edit = panel.edit.as_ref().expect("the editor opens");
        assert_eq!(edit.editor.pattern.name, "new-pattern");
        assert_eq!(
            edit.editor.pattern.kind,
            shepherd_app_core::shepherd_core::patterns::PatternKind::OrchestratorWorker
        );
        assert!(edit.editor.dirty, "a new pattern has something to save");
        assert!(panel
            .notice
            .starts_with("New pattern new-pattern from the orchestrator-worker template"));
        let _ = panel.patterns_view(&ctx);
        let delete = Armed::DeletePattern("new-pattern".into());
        let _ = panel.update(PanelMsg::Arm(delete.clone()), &ctx);
        assert_eq!(panel.armed, Some(delete));
    }

    #[test]
    fn the_communication_screen_opens_from_a_run_hides_senders_and_refuses_an_empty_send() {
        let data = Data::default();
        let snapshot = Snapshot::default();
        let ctx = ctx_of(&data, &snapshot);
        let mut panel = Panel::default();
        let _ = panel.update(PanelMsg::ShowMonitor("run-x".into()), &ctx);
        assert!(panel.comm_open());
        assert_eq!(panel.monitor.as_deref(), Some("run-x"));
        let _ = panel.update(PanelMsg::CommToggle("coder-1".into()), &ctx);
        assert!(panel.comm_hidden.contains("coder-1"));
        let _ = panel.update(PanelMsg::CommToggle("coder-1".into()), &ctx);
        assert!(panel.comm_hidden.is_empty());
        let _ = panel.view(&ctx);
        // no topic and no text: nothing is sent, the reason is a notice
        let _ = panel.update(PanelMsg::CommSend, &ctx);
        assert!(crate::ui::is_error(&panel.notice), "{}", panel.notice);
        // any view closes the screen again
        let _ = panel.update(PanelMsg::Tab(Tab::Agents), &ctx);
        assert!(!panel.comm_open());
        let _ = panel.update(PanelMsg::Tab(Tab::Monitor), &ctx);
        let _ = panel.view(&ctx);
    }

    #[test]
    fn a_refusal_carries_what_was_being_done_above_the_servers_words() {
        let data = Data::default();
        let snapshot = Snapshot::default();
        let ctx = ctx_of(&data, &snapshot);
        let mut panel = Panel::default();
        let _ = panel.update(PanelMsg::Arm(Armed::DeleteTask(4)), &ctx);
        let _ = panel.update(PanelMsg::Confirm, &ctx);
        let _ = panel.update(PanelMsg::Done(Err("task 4 is in a run".into())), &ctx);
        assert_eq!(panel.notice, "Not deleted.\ntask 4 is in a run");
        assert!(crate::ui::is_error(&panel.notice));
    }

    #[test]
    fn an_armed_button_disarms_after_its_time_or_on_esc() {
        let data = Data::default();
        let snapshot = Snapshot::default();
        let ctx = ctx_of(&data, &snapshot);
        let mut panel = Panel::default();
        let _ = panel.update(PanelMsg::Arm(Armed::CleanRuns), &ctx);
        panel.expire_armed();
        assert!(panel.armed.is_some(), "still within its seconds");
        panel.armed_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(5));
        panel.expire_armed();
        assert!(panel.armed.is_none(), "its time is up");
        let _ = panel.update(PanelMsg::Arm(Armed::CleanRuns), &ctx);
        panel.disarm();
        assert!(panel.armed.is_none() && panel.armed_at.is_none());
    }
}
