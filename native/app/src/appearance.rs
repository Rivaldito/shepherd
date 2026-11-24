//! The look the person picks from the gear menu: dark (the default), light, or minimal (the dark colours without
//! animation or decoration). Kept with the sizes in `native-view.json`.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
    Minimal,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Dark, Mode::Light, Mode::Minimal];

    pub fn key(self) -> &'static str {
        match self {
            Mode::Dark => "dark",
            Mode::Light => "light",
            Mode::Minimal => "minimal",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Dark => "Dark",
            Mode::Light => "Light",
            Mode::Minimal => "Minimal",
        }
    }

    fn from_key(key: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.key() == key)
    }
}

static MODE: AtomicU8 = AtomicU8::new(0);

pub fn mode() -> Mode {
    Mode::ALL[MODE.load(Ordering::Relaxed) as usize]
}

pub fn is_light() -> bool {
    mode() == Mode::Light
}

pub fn is_minimal() -> bool {
    mode() == Mode::Minimal
}

/// Switches the look and keeps it for the next run.
pub fn set(mode: Mode) {
    store(mode);
    crate::zoom::save();
}

fn store(mode: Mode) {
    let index = Mode::ALL.iter().position(|m| *m == mode).unwrap_or(0);
    MODE.store(index as u8, Ordering::Relaxed);
}

/// Reads the look of the last run from the state file's `theme` entry; anything else stays dark.
/// `SHEPHERD_NATIVE_THEME=light|minimal` overrides it (for pictures).
pub fn load(saved: Option<&str>) {
    let from_env = std::env::var("SHEPHERD_NATIVE_THEME").ok();
    if let Some(mode) = from_env.as_deref().or(saved).and_then(Mode::from_key) {
        store(mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_is_found_by_its_key_and_unknown_keys_are_ignored() {
        for mode in Mode::ALL {
            assert_eq!(Mode::from_key(mode.key()), Some(mode));
        }
        assert_eq!(Mode::from_key("neon"), None);
    }
}
