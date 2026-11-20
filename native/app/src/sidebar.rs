//! The sidebar's derived values (spec 4.2.5, 4.2.7, 4.2.9): pure, so they are tested without a window.
//! The drawing is in `main.rs`; this file only decides what the rows say.

use crate::theme;
use iced::Color;
use shepherd_app_core::snapshot::Status;

/// Widths of the sidebar's regions (spec 4.2.3).
pub const WIDTH: f32 = 232.0;
pub const NAV_H: f32 = 387.0;
pub const WS_ROW_H: f32 = 32.0;
pub const AGENT_ROW_H: f32 = 28.0;
/// Under this window height the whole column scrolls as one piece (spec 4.2.3: a list viewport under 120 px).
pub const SHORT_WINDOW: f32 = 711.0;

/// What a workspace row shows: the square, the label and the label's colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Roll {
    pub square: Color,
    pub label: String,
    pub label_color: Color,
}

/// One agent pane of a workspace: its state and whether the human is needed.
#[derive(Debug, Clone, Copy)]
pub struct AgentPane {
    pub status: Status,
    pub needs_human: bool,
}

impl AgentPane {
    /// Blocked, or flagged as needing the human.
    pub fn urgent(&self) -> bool {
        self.status == Status::Blocked || self.needs_human
    }
}

/// The most urgent state of a workspace (spec 4.2.5, rules in this order).
pub fn rollup(agents: &[AgentPane], shells: usize) -> Roll {
    let urgent = agents.iter().filter(|a| a.urgent()).count();
    let working = agents
        .iter()
        .filter(|a| !a.urgent() && a.status == Status::Working)
        .count();
    if urgent > 0 {
        Roll {
            square: theme::WARN,
            label: format!("{urgent} needs you"),
            label_color: theme::WARN,
        }
    } else if working > 0 {
        Roll {
            square: theme::ACCENT,
            label: format!("{working} working"),
            label_color: theme::DIM,
        }
    } else if !agents.is_empty() {
        Roll {
            square: theme::IDLE,
            label: "idle".into(),
            label_color: theme::DIM,
        }
    } else if shells > 0 {
        Roll {
            square: theme::LINE2,
            label: if shells == 1 {
                "1 shell".into()
            } else {
                format!("{shells} shells")
            },
            label_color: theme::FAINT,
        }
    } else {
        Roll {
            square: theme::LINE2,
            label: "empty".into(),
            label_color: theme::FAINT,
        }
    }
}

/// Shortens `s` to `max_chars` characters with a final "…".
pub fn ellipsize(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_owned()
    } else {
        s.chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>()
            + "…"
    }
}

/// Workspace name: the 161 px button − 8 padding − 7 square − 2 × 8 gaps = 130 px, minus the label
/// (JetBrains Mono advances 0.6 em: 7.5 px at 12.5, 6.3 px at 10.5).
pub fn ws_name_max_chars(label: &str) -> usize {
    let label_px = label.chars().count() as f32 * 6.3;
    ((130.0 - label_px) / 7.5).floor().max(3.0) as usize
}

/// Agent name: the 191 px row − 2 border − 12 padding − 15 icon − 2 × 8 gaps = 146 px, minus the label.
pub fn agent_name_max_chars(label: &str) -> usize {
    let label_px = label.chars().count() as f32 * 6.3;
    ((146.0 - label_px) / 7.5).floor().max(3.0) as usize
}

/// What closing a workspace asks for (spec 4.2.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseRule {
    /// Agents still run in it: refused, with this text.
    Refused(String),
    /// Only plain shells: two clicks, the armed button says this.
    Confirm(String),
    /// Nothing in it: closes at once.
    Now,
}

pub fn close_rule(name: &str, agents: usize, panes: usize) -> CloseRule {
    if agents > 0 {
        CloseRule::Refused(format!(
            "Workspace {name} still runs {agents} agent{}. Close them first, then the workspace.",
            if agents == 1 { "" } else { "s" }
        ))
    } else if panes > 0 {
        CloseRule::Confirm(format!(
            "Confirm: close {panes} terminal{}",
            if panes == 1 { "" } else { "s" }
        ))
    } else {
        CloseRule::Now
    }
}

/// The plain-shell line of an open workspace: "+ N plain shell(s)" under agent rows, "N plain shell(s)" alone.
pub fn shells_line(shells: usize, agent_rows: usize) -> Option<String> {
    if shells == 0 {
        return None;
    }
    let noun = if shells == 1 { "shell" } else { "shells" };
    Some(if agent_rows > 0 {
        format!("+ {shells} plain {noun}")
    } else {
        format!("{shells} plain {noun}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(status: Status, needs_human: bool) -> AgentPane {
        AgentPane {
            status,
            needs_human,
        }
    }

    #[test]
    fn the_roll_up_is_the_most_urgent_state_in_the_order_of_the_spec() {
        let r = rollup(
            &[
                agent(Status::Working, false),
                agent(Status::Blocked, false),
                agent(Status::Idle, true),
            ],
            2,
        );
        assert_eq!((r.label.as_str(), r.square), ("2 needs you", theme::WARN));
        let r = rollup(
            &[agent(Status::Working, false), agent(Status::Idle, false)],
            0,
        );
        assert_eq!((r.label.as_str(), r.square), ("1 working", theme::ACCENT));
        let r = rollup(&[agent(Status::Idle, false)], 3);
        assert_eq!(r.label, "idle");
        assert_eq!(rollup(&[], 1).label, "1 shell");
        assert_eq!(rollup(&[], 2).label, "2 shells");
        assert_eq!(rollup(&[], 0).label, "empty");
    }

    #[test]
    fn long_names_end_in_an_ellipsis_by_the_character_counts_of_the_spec() {
        // the spec's example: the label "1 needs you" leaves 8 characters
        assert_eq!(ws_name_max_chars("1 needs you"), 8);
        assert_eq!(ellipsize("alpha", 8), "alpha");
        assert_eq!(ellipsize("frontend-redesign", 8), "fronten…");
        assert_eq!(ws_name_max_chars(&"x".repeat(40)), 3, "never under 3");
        assert!(agent_name_max_chars("working") > ws_name_max_chars("working"));
    }

    #[test]
    fn closing_follows_the_rules_of_the_spec() {
        assert!(matches!(close_rule("alpha", 2, 5), CloseRule::Refused(t)
            if t == "Workspace alpha still runs 2 agents. Close them first, then the workspace."));
        assert_eq!(
            close_rule("gamma", 0, 2),
            CloseRule::Confirm("Confirm: close 2 terminals".into())
        );
        assert_eq!(close_rule("beta", 0, 0), CloseRule::Now);
    }

    #[test]
    fn plain_shells_are_counted_not_listed() {
        assert_eq!(shells_line(1, 3).as_deref(), Some("+ 1 plain shell"));
        assert_eq!(shells_line(2, 0).as_deref(), Some("2 plain shells"));
        assert_eq!(shells_line(0, 3), None);
    }
}
