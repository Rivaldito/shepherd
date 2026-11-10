//! Build identity helpers.

pub const BASE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The herdr release this fork was last synced with. Shepherd has its own version
/// (`BASE_VERSION`); `scripts/upstream-sync.sh` updates this line on every sync.
pub const UPSTREAM_VERSION: &str = "0.9.5";

pub fn channel() -> &'static str {
    non_empty(option_env!("SHEPHERD_BUILD_CHANNEL")).unwrap_or("stable")
}

pub fn build_id() -> Option<&'static str> {
    non_empty(option_env!("SHEPHERD_BUILD_ID"))
}

pub fn version() -> String {
    match channel() {
        "stable" => BASE_VERSION.to_string(),
        channel => match build_id() {
            Some(build_id) => format!("{BASE_VERSION}-{channel}.{build_id}"),
            None => format!("{BASE_VERSION}-{channel}"),
        },
    }
}

/// Text printed by `shepherd --version`.
pub fn version_line() -> String {
    format!("shepherd {} (herdr {UPSTREAM_VERSION})", version())
}

pub fn is_preview() -> bool {
    channel() == "preview"
}

fn non_empty(value: Option<&'static str>) -> Option<&'static str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn stable_version_defaults_to_cargo_version() {
        assert!(!super::version().is_empty());
    }

    #[test]
    fn cargo_version_is_numeric_so_the_updater_can_parse_it() {
        assert!(crate::update::Version::parse(super::BASE_VERSION).is_some());
        assert!(crate::update::Version::parse(super::UPSTREAM_VERSION).is_some());
        assert_ne!(super::BASE_VERSION, super::UPSTREAM_VERSION);
    }

    #[test]
    fn version_line_names_shepherd_and_the_herdr_base() {
        let line = super::version_line();
        assert!(line.starts_with(&format!("shepherd {}", super::version())));
        assert!(line.ends_with(&format!("(herdr {})", super::UPSTREAM_VERSION)));
    }
}
