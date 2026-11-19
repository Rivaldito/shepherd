//! What an agent did over a window of time, from the state changes the server records: the segments Activity
//! draws as one bar per agent. Pure, so it is tested without a window.

use shepherd_core::StateChange;

/// State word of a stretch the log says nothing about.
pub const UNKNOWN: &str = "unknown";

/// `(state, seconds)` segments of `agent` from `from` to `to` (unix seconds), oldest first. A stretch before the
/// agent's first recorded change is `unknown`; an agent with no record at all was `current` all along as far as is
/// known.
pub fn segments(
    log: &[StateChange],
    agent: &str,
    current: &str,
    from: u64,
    to: u64,
) -> Vec<(String, u64)> {
    if to <= from {
        return Vec::new();
    }
    let mine: Vec<&StateChange> = log.iter().filter(|c| c.agent == agent).collect();
    if mine.is_empty() {
        return vec![(current.to_owned(), to - from)];
    }
    let mut state = mine
        .iter()
        .rev()
        .find(|c| c.t <= from)
        .map_or(UNKNOWN.to_owned(), |c| c.state.clone());
    let mut at = from;
    let mut out: Vec<(String, u64)> = Vec::new();
    let mut push = |state: &str, secs: u64| {
        if secs == 0 {
            return;
        }
        match out.last_mut() {
            Some((s, d)) if s == state => *d += secs,
            _ => out.push((state.to_owned(), secs)),
        }
    };
    for c in mine.iter().filter(|c| c.t > from && c.t < to) {
        push(&state, c.t - at);
        state = c.state.clone();
        at = c.t;
    }
    push(&state, to - at);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(t: u64, state: &str) -> StateChange {
        StateChange {
            t,
            agent: "a".into(),
            state: state.into(),
        }
    }

    #[test]
    fn segments_follow_the_changes_and_merge_equal_neighbours() {
        let log = vec![change(50, "working"), change(120, "idle"), change(150, "idle"), change(180, "working")];
        // window 100..200: working until 120, idle 120..180, working 180..200
        assert_eq!(
            segments(&log, "a", "working", 100, 200),
            [("working".into(), 20), ("idle".into(), 60), ("working".into(), 20)]
        );
    }

    #[test]
    fn before_the_first_record_is_unknown_and_no_record_means_the_current_state() {
        let log = vec![change(150, "working")];
        assert_eq!(
            segments(&log, "a", "working", 100, 200),
            [(UNKNOWN.into(), 50), ("working".into(), 50)]
        );
        assert_eq!(segments(&[], "a", "idle", 100, 200), [("idle".into(), 100)]);
        assert!(segments(&log, "a", "idle", 200, 200).is_empty());
    }
}
