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
                    let agent = agent.clone();
                    if c.created {
                        format!("Took snapshot {} of {agent}’s worktree.", c.snapshot.id)
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

    /// The agent the Capture box of the header names: the picked one, else the shown snapshot's, else the first.
    pub(super) fn capture_agent(&self, ctx: &Ctx<'_>) -> Option<String> {
        self.snap_capture_agent
            .clone()
            .filter(|a| ctx.data.agents.iter().any(|x| &x.id == a))
            .or_else(|| {
                self.snap_id(ctx)
                    .and_then(|id| ctx.data.snapshots.iter().find(|s| s.id == id))
                    .map(|s| s.agent.clone())
            })
            .or_else(|| ctx.data.agents.first().map(|a| a.id.clone()))
    }

    pub(super) fn snapshots_view<'a>(&'a self, ctx: &Ctx<'a>) -> Element<'a, Message> {
        let snaps = &ctx.data.snapshots;
        let shown = self.snap_id(ctx);
        let mut agents: Vec<String> = vec![];
        for s in snaps {
            if !agents.contains(&s.agent) {
                agents.push(s.agent.clone());
            }
        }
        let reasons = ["manual", "idle", "done", "pre-restore"];
        let chip = |label: String, on: bool, msg: PanelMsg| {
            button(text(label).size(11.5))
                .padding([3, 10])
                .style(theme::chip_style(on))
                .on_press(Message::Panel(msg))
        };
        let mut agent_chips = row![chip(
            "All agents".into(),
            self.snap_agent_filter.is_none(),
            PanelMsg::SnapFilterAgent(None)
        )]
        .spacing(6);
        for a in &agents {
            agent_chips = agent_chips.push(chip(
                a.clone(),
                self.snap_agent_filter.as_deref() == Some(a.as_str()),
                PanelMsg::SnapFilterAgent(Some(a.clone())),
            ));
        }
        let mut reason_chips = row![chip(
            "Any reason".into(),
            self.snap_reason_filter.is_none(),
            PanelMsg::SnapFilterReason(None)
        )]
        .spacing(6);
        for r in reasons {
            reason_chips = reason_chips.push(chip(
                r.to_owned(),
                self.snap_reason_filter.as_deref() == Some(r),
                PanelMsg::SnapFilterReason(Some(r.to_owned())),
            ));
        }
        let filters = container(
            row![
                text_input("Filter by agent or id", &self.filter)
                    .on_input(|t| Message::Panel(PanelMsg::Filter(t)))
                    .id(crate::ui::FILTER_ID)
                    .style(theme::input_style)
                    .padding([8, 10])
                    .size(12)
                    .width(260),
                agent_chips.wrap(),
                container(text(""))
                    .width(1)
                    .height(20)
                    .style(|_| container::Style {
                        background: Some(Background::Color(theme::LINE2)),
                        ..container::Style::default()
                    }),
                reason_chips.wrap(),
            ]
            .spacing(14)
            .align_y(iced::Alignment::Center),
        )
        .padding([10, 20])
        .width(Length::Fill)
        .style(theme::bottom_line_style);

        let visible = |s: &&SnapshotInfo| {
            self.snap_agent_filter
                .as_deref()
                .is_none_or(|a| a == s.agent)
                && self
                    .snap_reason_filter
                    .as_deref()
                    .is_none_or(|r| r == s.reason.as_str())
                && self.passes(&format!("{} {}", s.id, s.agent))
        };
        // grouped by agent (newest first inside), each group with its own prune
        let mut items: Vec<Element<'a, Message>> = vec![];
        for agent in &agents {
            let mine: Vec<&SnapshotInfo> = snaps
                .iter()
                .filter(|s| &s.agent == agent)
                .filter(visible)
                .collect();
            if mine.is_empty() {
                continue;
            }
            items.push(
                row![
                    text(agent.clone())
                        .font(theme::MONO_BOLD)
                        .size(12.5)
                        .color(theme::HI),
                    text(format!("{} snapshots", mine.len()))
                        .size(11)
                        .color(DIM)
                        .width(Length::Fill),
                    self.arm_button(
                        Armed::PruneSnapshots(agent.clone()),
                        "Prune older",
                        "Confirm: prune"
                    ),
                ]
                .spacing(8)
                .padding(pad(10.0, 4.0, 4.0, 4.0))
                .align_y(iced::Alignment::Center)
                .into(),
            );
            for s in mine {
                let sel = shown.as_deref() == Some(s.id.as_str());
                let rc = reason_color(s.reason);
                let body = row![
                    // a line the items hang on, and a ring in the reason colour
                    container(text(""))
                        .width(1)
                        .height(Length::Fill)
                        .style(|_| container::Style {
                            background: Some(iced::Background::Color(theme::LINE2)),
                            ..container::Style::default()
                        }),
                    container(text(""))
                        .width(11)
                        .height(11)
                        .style(move |_| container::Style {
                            border: iced::Border {
                                color: if sel { theme::HI } else { rc },
                                width: 2.0,
                                radius: 6.0.into(),
                            },
                            ..container::Style::default()
                        }),
                    column![
                        row![
                            text(s.id.clone())
                                .font(theme::MONO_BOLD)
                                .size(12.5)
                                .color(theme::HI),
                            container(text(s.reason.as_str()).size(10).color(rc))
                                .padding([0, 6])
                                .style(move |_| container::Style {
                                    border: iced::Border {
                                        color: rc,
                                        width: 1.0,
                                        radius: 3.0.into()
                                    },
                                    ..container::Style::default()
                                }),
                            container(text("")).width(Length::Fill),
                            text(
                                format_utc(s.created)
                                    .chars()
                                    .skip(11)
                                    .take(5)
                                    .collect::<String>()
                            )
                            .size(11)
                            .color(DIM),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                        row![
                            text(format!("{} files", s.files)).size(11).color(DIM),
                            text(format!("+{}", s.added)).size(11).color(OK),
                            text(format!("-{}", s.removed)).size(11).color(BAD),
                        ]
                        .spacing(8),
                    ]
                    .spacing(3)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .height(40)
                .align_y(iced::Alignment::Center);
                items.push(
                    button(body)
                        .width(Length::Fill)
                        .padding([8, 10])
                        .style(item_style(sel))
                        .on_press(crate::ui::nav(
                            sel,
                            Message::Panel(PanelMsg::SnapSelect(s.id.clone())),
                        ))
                        .into(),
                );
            }
        }
        if items.is_empty() && !snaps.is_empty() {
            items.push(text("No snapshot matches.").size(12).color(DIM).into());
        }

        // the detail is the flexible region: at least 600 px, scrolling when narrower
        let detail = {
            let (this, ctx) = (self, ctx.clone());
            crate::ui::flex(600.0, [22.0, 28.0], move || {
                let shown = this.snap_id(&ctx);
                match shown
                    .as_deref()
                    .and_then(|id| ctx.data.snapshots.iter().find(|s| s.id == id))
                {
                    Some(snap) => this.snapshot_detail(snap),
                    None => column![
                        text("No snapshots match").size(16).color(theme::HI),
                        text(
                            "The server takes one when an agent goes idle or finishes, and before every restore."
                        )
                        .size(12)
                        .color(DIM),
                        btn_primary("Capture now", PanelMsg::SnapCapture(
                            ctx.data.agents.first().map(|a| a.id.clone()).unwrap_or_default()
                        )),
                    ]
                    .spacing(8)
                    .into(),
                }
            })
        };
        column![
            filters,
            row![
                crate::panel::list_box_sized(400.0, None, items, vec![]),
                detail
            ]
            .height(Length::Fill),
        ]
        .into()
    }

    fn snapshot_detail<'a>(&'a self, snap: &'a SnapshotInfo) -> Element<'a, Message> {
        let mut page = column![
            row![
                text(snap.id.clone()).font(theme::MONO_BOLD).size(20).color(theme::HI),
                badge(snap.reason.as_str(), reason_color(snap.reason)),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            text(format!(
                "taken {}   worktree {}   previous {}",
                format_utc(snap.created),
                snap.worktree,
                snap.prev.clone().unwrap_or_else(|| "none".into())
            ))
            .size(11.5)
            .color(DIM),
            row![
                btn_primary("Compare with the worktree now", PanelMsg::SnapCompare(snap.id.clone())),
                self.arm_button(
                    Armed::RestoreSnapshot(snap.id.clone()),
                    "Restore worktree",
                    "Confirm: restore the whole worktree",
                ),
                self.arm_button(
                    Armed::DeleteSnapshot(snap.id.clone()),
                    "Delete",
                    "Confirm: delete this snapshot",
                ),
            ]
            .spacing(8),
            text(
                "Restore puts the whole worktree back as this snapshot had it. A pre-restore snapshot of what is \
                 there now is taken first, and files this snapshot does not have are kept.",
            )
            .size(11.5)
            .color(DIM),
        ]
        .spacing(10);

        // the file list of a diff: status, path, +A -D
        let files = |files: &'a [shepherd_app_core::shepherd_core::snapshot::DiffFile]| {
            let mut list = column![].spacing(6);
            for f in files {
                list = list.push(
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
            list
        };

        let since = match &snap.prev {
            Some(prev) => format!("since {prev}: "),
            None => "first snapshot of this agent: ".to_owned(),
        };
        let mut body = column![].spacing(8);
        match self.snap_detail.as_ref().filter(|(id, _)| *id == snap.id) {
            None => body = body.push(text("Loading the changed files…").size(12).color(DIM)),
            Some((_, Err(e))) => {
                body = body.push(alert(
                    text(format!("Could not read the snapshot: {e}"))
                        .size(12)
                        .into(),
                    BAD,
                ))
            }
            Some((_, Ok(shown))) => {
                if shown.diff.files.is_empty() {
                    body = body.push(
                        text("No file changed since the previous snapshot.")
                            .size(12)
                            .color(DIM),
                    );
                }
                body = body.push(files(&shown.diff.files));
            }
        }
        if !snap.omitted.is_empty() {
            body = body.push(
                text(format!(
                    "{} file(s) not listed: {}",
                    snap.omitted.len(),
                    snap.omitted
                        .iter()
                        .map(|o| format!("{} ({}, {} bytes)", o.path, o.reason, o.size))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .size(11)
                .color(WARN),
            );
        }
        page = page.push(card(
            "CHANGES",
            Some(
                text(format!(
                    "{since}{} files +{} −{}",
                    snap.files, snap.added, snap.removed
                ))
                .size(11)
                .color(DIM)
                .into(),
            ),
            body.into(),
        ));

        match self.snap_compare.as_ref().filter(|(id, _)| *id == snap.id) {
            None => {}
            Some((_, Err(e))) => {
                page = page.push(alert(
                    text(format!("Could not compare: {e}")).size(12).into(),
                    BAD,
                ))
            }
            Some((_, Ok(diff))) => {
                let mut body = column![].spacing(8);
                if diff.files.is_empty() {
                    body = body.push(
                        text("The worktree is the same as this snapshot.")
                            .size(12)
                            .color(DIM),
                    );
                }
                body = body.push(files(&diff.files));
                if let Some(patch) = diff.patch.as_ref().filter(|p| !p.trim().is_empty()) {
                    body = body.push(diff_excerpt(patch));
                }
                page = page.push(card(
                    "WORKTREE NOW COMPARED WITH THIS SNAPSHOT",
                    Some(
                        row![
                            text(snap.id.clone()).size(11).color(DIM),
                            button(text("Hide").size(11.5))
                                .padding([3, 10])
                                .style(theme::outline_button_style(false))
                                .on_press(Message::Panel(PanelMsg::SnapHide)),
                        ]
                        .spacing(10)
                        .align_y(iced::Alignment::Center)
                        .into(),
                    ),
                    body.into(),
                ));
            }
        }
        page.into()
    }
}

/// The first lines of a unified diff: hunk headers dim, removed lines red, added lines green.
fn diff_excerpt<'a>(patch: &str) -> Element<'a, Message> {
    const SHOWN: usize = 60;
    let mut lines = column![].spacing(1);
    let all: Vec<&str> = patch
        .lines()
        .filter(|l| !(l.starts_with("diff --git") || l.starts_with("index ")))
        .collect();
    for l in all.iter().take(SHOWN) {
        let color = if l.starts_with("@@") || l.starts_with("---") || l.starts_with("+++") {
            DIM
        } else if l.starts_with('-') {
            BAD
        } else if l.starts_with('+') {
            OK
        } else {
            theme::TEXT2
        };
        lines = lines.push(text((*l).to_owned()).size(11).color(color));
    }
    if all.len() > SHOWN {
        lines = lines.push(
            text(format!("… {} more lines", all.len() - SHOWN))
                .size(11)
                .color(DIM),
        );
    }
    container(lines)
        .padding([8, 10])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(theme::INPUT)),
            border: iced::Border {
                color: theme::LINE,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}
