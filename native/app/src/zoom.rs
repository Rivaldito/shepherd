//! The two sizes (and, with them, the look) the person sets from the gear menu: the interface (the whole window, text included) and the
//! terminal (its text and cells, on top of the interface size). Percentages, kept in the state dir between runs.

use shepherd_app_core::shepherd_core::config::state_dir;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

const STEP: u32 = 10;
const INTERFACE: Range = Range { min: 80, max: 200 };
const TERMINAL: Range = Range { min: 60, max: 250 };

struct Range {
    min: u32,
    max: u32,
}

/// Which of the two sizes a control changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Interface,
    Terminal,
}

/// What a control does to a size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Smaller,
    Bigger,
    Reset,
}

static INTERFACE_PERCENT: AtomicU32 = AtomicU32::new(100);
static TERMINAL_PERCENT: AtomicU32 = AtomicU32::new(100);

fn cell(target: Target) -> &'static AtomicU32 {
    match target {
        Target::Interface => &INTERFACE_PERCENT,
        Target::Terminal => &TERMINAL_PERCENT,
    }
}

fn range(target: Target) -> &'static Range {
    match target {
        Target::Interface => &INTERFACE,
        Target::Terminal => &TERMINAL,
    }
}

/// The size of `target`, in percent.
pub fn percent(target: Target) -> u32 {
    cell(target).load(Ordering::Relaxed)
}

/// The interface scale (1.0 = 100 %).
pub fn interface() -> f32 {
    percent(Target::Interface) as f32 / 100.0
}

/// The terminal zoom (1.0 = 100 %).
pub fn terminal() -> f32 {
    percent(Target::Terminal) as f32 / 100.0
}

/// `current` after `change`, kept inside the range of `target`.
fn changed(target: Target, current: u32, change: Change) -> u32 {
    let range = range(target);
    match change {
        Change::Reset => 100,
        Change::Smaller => current.saturating_sub(STEP).max(range.min),
        Change::Bigger => (current + STEP).min(range.max),
    }
}

/// Applies `change` to `target` and keeps the result for the next run.
pub fn apply(target: Target, change: Change) {
    let size = cell(target);
    size.store(
        changed(target, size.load(Ordering::Relaxed), change),
        Ordering::Relaxed,
    );
    save();
}

fn file() -> PathBuf {
    state_dir().join("native-view.json")
}

/// Reads the sizes of the last run; anything missing or out of range stays at 100 %.
/// `SHEPHERD_NATIVE_SIZES=120,150` (interface, terminal) overrides them for pictures (`make native-shot`).
pub fn load() {
    if let Some((i, t)) = std::env::var("SHEPHERD_NATIVE_SIZES")
        .ok()
        .as_deref()
        .and_then(|v| v.split_once(','))
    {
        for (target, value) in [(Target::Interface, i), (Target::Terminal, t)] {
            if let Ok(p) = value.trim().parse::<u32>() {
                cell(target).store(
                    p.clamp(range(target).min, range(target).max),
                    Ordering::Relaxed,
                );
            }
        }
        crate::appearance::load(None);
        return;
    }
    let json = std::fs::read_to_string(file())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or_default();
    crate::appearance::load(json["theme"].as_str());
    for (target, key) in [
        (Target::Interface, "interface"),
        (Target::Terminal, "terminal"),
    ] {
        let range = range(target);
        if let Some(p) = json[key]
            .as_u64()
            .map(|p| p as u32)
            .filter(|p| (range.min..=range.max).contains(p))
        {
            cell(target).store(p, Ordering::Relaxed);
        }
    }
}

pub(crate) fn save() {
    let json = serde_json::json!({
        "interface": percent(Target::Interface),
        "terminal": percent(Target::Terminal),
        "theme": crate::appearance::mode().key(),
    });
    // a size that cannot be kept is only forgotten at the next run
    if let Some(dir) = file().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(file(), json.to_string()) {
        eprintln!("could not keep the sizes: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_moves_by_steps_and_stops_at_the_ends_of_its_range() {
        assert_eq!(changed(Target::Interface, 100, Change::Bigger), 110);
        assert_eq!(changed(Target::Interface, 100, Change::Smaller), 90);
        assert_eq!(changed(Target::Interface, 200, Change::Bigger), 200);
        assert_eq!(changed(Target::Interface, 80, Change::Smaller), 80);
        assert_eq!(changed(Target::Terminal, 60, Change::Smaller), 60);
        assert_eq!(changed(Target::Terminal, 250, Change::Bigger), 250);
        assert_eq!(changed(Target::Terminal, 170, Change::Reset), 100);
    }
}
