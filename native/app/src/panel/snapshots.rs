//! The Snapshots tab: the copies of each agent's worktree that the server takes (automatically and on
//! request), what each one changed, and putting one back. The server does all of it; this only asks.

use super::*;
use iced::widget::column;
use shepherd_app_core::shepherd_core::snapshot::{format_utc, SnapshotInfo, SnapshotReason};

fn reason_color(reason: SnapshotReason) -> Color {
    match reason {
        SnapshotReason::Manual => ACCENT,
        SnapshotReason::Done => OK,
        SnapshotReason::PreRestore | SnapshotReason::PreClean | SnapshotReason::PreKill => WARN,
        SnapshotReason::Idle | SnapshotReason::Periodic => DIM,
    }
}

fn change_color(status: &str) -> Color {
    match status {
        "added" => OK,
        "deleted" => BAD,
        _ => WARN,
    }
}

/// The directory to snapshot for an agent: the server copies the directory it is given and only uses
/// the agent's name as a label, so this is the agent's own worktree (else its project).
pub(super) fn capture_root(data: &Data, agent: &str) -> String {
    data.agents
        .iter()
        .find(|a| a.id == agent)
        .and_then(|a| [&a.worktree, &a.root].into_iter().find(|p| !p.is_empty()))
        .cloned()
        .unwrap_or_else(|| data.root.display().to_string())
}

/// The directory a snapshot is put back into: the worktree it was taken in.
pub(super) fn restore_root(data: &Data, id: &str) -> String {
    data.snapshots
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.worktree.clone())
        .filter(|w| !w.is_empty())
        .unwrap_or_else(|| data.root.display().to_string())
}

impl Panel {
    /// The snapshot the tab shows: the picked one if it still exists, else the newest.
    pub(super) fn snap_id(&self, ctx: &Ctx<'_>) -> Option<String> {
        let snaps = &ctx.data.snapshots;
        self.snap
            .as_ref()
            .filter(|id| snaps.iter().any(|s| &s.id == *id))
            .or_else(|| snaps.first().map(|s| &s.id))
            .cloned()
    }

    /// Asks the server what the shown snapshot holds.
    pub(super) fn snap_show(&self, ctx: &Ctx<'_>) -> Task<Message> {
        let Some(id) = self.snap_id(ctx) else {
            return Task::none();
        };
        let api = ctx.api.clone();
        let root = ctx.data.root.display().to_string();
        Task::perform(
            async move {
                let shown = actions::snapshot_show(&api, &id, &root).map(Box::new);
                (id, shown)
            },
            |(id, shown)| Message::Panel(PanelMsg::SnapShown(id, shown)),
        )
    }

    pub(super) fn snap_capture(&self, agent: String, ctx: &Ctx<'_>) -> Task<Message> {
        let api = ctx.api.clone();
        let root = capture_root(ctx.data, &agent);
        Task::perform(
            async move {
                actions::snapshot_capture(&api, &agent, &root).map(|c| {
                    if c.created {
                        format!("Took snapshot {}", c.snapshot.id)
                    } else {
                        format!(
                            "Nothing changed since {}, so no new snapshot",
                            c.snapshot.id
                        )
                    }
                })
            },
            |r| Message::Panel(PanelMsg::Done(r)),
        )
    }

    pub(super) fn snapshots_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let snaps = &ctx.data.snapshots;
        let shown = self.snap_id(ctx);
        let agents = snaps
            .iter()
            .map(|s| s.agent.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let (files, added, removed) = snaps.iter().fold((0u64, 0u64, 0u64), |(f, a, r), s| {
            (f + s.files as u64, a + s.added, r + s.removed)
        });
        let summary = container(
            row![
                agent_metric("SNAPSHOTS", snaps.len(), FG),
                agent_metric("AGENTS", agents, ACCENT),
                agent_metric("FILES CHANGED", files as usize, DIM),
                agent_metric("LINES +", added as usize, OK),
                agent_metric(
                    "LINES -",
                    removed as usize,
                    if removed > 0 { WARN } else { DIM }
                ),
            ]
            .spacing(1),
        )
        .style(kpi_strip_style);

        let items = snaps
            .iter()
            .map(|s| {
                let body = column![
                    row![
                        text(s.id.clone()).size(13).color(theme::HI),
                        badge(s.reason.as_str(), reason_color(s.reason)),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                    text(format!("{} file(s) · +{} −{}", s.files, s.added, s.removed))
                        .size(11)
                        .color(DIM),
                    text(format_utc(s.created)).size(10).color(DIM)
                ];
                button(body)
                    .width(Length::Fill)
                    .padding([4, 8])
                    .style(item_style(shown.as_deref() == Some(s.id.as_str())))
                    .on_press(Message::Panel(PanelMsg::SnapSelect(s.id.clone())))
                    .into()
            })
            .collect();

        // Capturing needs an agent: the one of the shown snapshot, else the first agent that exists.
        let capture_agent = shown
            .as_deref()
            .and_then(|id| snaps.iter().find(|s| s.id == id))
            .map(|s| s.agent.clone())
            .or_else(|| ctx.data.agents.first().map(|a| a.id.clone()));
        let footer = capture_agent
            .map(|agent| {
                vec![btn_link(
                    format!("+ Capture {agent} now"),
                    PanelMsg::SnapCapture(agent),
                )]
            })
            .unwrap_or_default();

        let detail = match shown.as_deref().and_then(|id| snaps.iter().find(|s| s.id == id)) {
            Some(snap) => self.snapshot_detail(snap),
            None => column![
                text("No snapshots yet").size(16).color(theme::HI),
                text(
                    "The server takes one of an agent's worktree when the agent goes idle or finishes, \
                     and now and then while it works. Use Capture to take one yourself."
                )
                .size(12)
                .color(DIM),
            ]
            .spacing(6)
            .into(),
        };
        column![
            summary,
            row![self.list(items, footer), self.detail(detail)].height(Length::Fill),
        ]
        .spacing(8)
        .into()
    }

    fn snapshot_detail<'a>(&'a self, snap: &'a SnapshotInfo) -> Element<'a, Message> {
        let mut page = column![
            row![
                text(snap.id.clone()).size(18).color(theme::HI),
                badge(snap.reason.as_str(), reason_color(snap.reason)),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            text(format!(
                "{} · commit {} · worktree {}",
                format_utc(snap.created),
                snap.commit.chars().take(8).collect::<String>(),
                snap.worktree
            ))
            .size(11)
            .color(DIM),
            text(match &snap.prev {
                Some(prev) => format!(
                    "Changes since {prev}: {} file(s), +{} −{}",
                    snap.files, snap.added, snap.removed
                ),
                None => format!(
                    "First snapshot of this agent: {} file(s), +{} −{}",
                    snap.files, snap.added, snap.removed
                ),
            })
            .size(12)
            .color(FG),
        ]
        .spacing(6);

        match self.snap_detail.as_ref().filter(|(id, _)| *id == snap.id) {
            None => page = page.push(text("Loading the changed files…").size(12).color(DIM)),
            Some((_, Err(e))) => {
                page = page.push(alert(
                    text(format!("Could not read the snapshot: {e}"))
                        .size(12)
                        .into(),
                    BAD,
                ))
            }
            Some((_, Ok(shown))) => {
                if shown.diff.files.is_empty() {
                    page = page.push(
                        text("No file changed since the previous snapshot.")
                            .size(12)
                            .color(DIM),
                    );
                }
                for f in &shown.diff.files {
                    page = page.push(
                        row![
                            container(
                                text(f.status.clone())
                                    .size(11)
                                    .color(change_color(&f.status))
                            )
                            .width(90),
                            text(f.path.clone()).size(12).color(FG).width(Length::Fill),
                            text(if f.binary {
                                "binary".to_owned()
                            } else {
                                format!("+{} −{}", f.added, f.removed)
                            })
                            .size(11)
                            .color(DIM),
                        ]
                        .spacing(8),
                    );
                }
            }
        }
        for o in &snap.omitted {
            page = page.push(
                text(format!(
                    "Left out: {} ({}, {} bytes)",
                    o.path, o.reason, o.size
                ))
                .size(11)
                .color(WARN),
            );
        }
        page.push(
            text(
                "Restore puts the whole worktree back as this snapshot had it. A snapshot of what is there now \
                 is taken first, and files this snapshot does not have are kept.",
            )
            .size(11)
            .color(DIM),
        )
        .push(
            row![
                self.arm_button(
                    Armed::RestoreSnapshot(snap.id.clone()),
                    "Restore worktree",
                    "Confirm: restore the whole worktree",
                ),
                self.arm_button(
                    Armed::DeleteSnapshot(snap.id.clone()),
                    "Delete snapshot",
                    "Confirm: delete this snapshot",
                ),
            ]
            .spacing(8),
        )
        .spacing(10)
        .into()
    }
}
