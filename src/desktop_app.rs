//! Opening the desktop app: `shepherd --gui`, and `shepherd` alone when the app can open. The app is its own
//! program (`shepherd-native`, installed next to `shepherd`); this only decides which interface a launch starts and
//! starts the app, detached, so the terminal that ran the command is free again.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(windows)]
const APP_BINARY: &str = "shepherd-native.exe";
#[cfg(not(windows))]
const APP_BINARY: &str = "shepherd-native";

/// What the arguments ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    Gui,
    Tui,
    /// Neither flag: the machine decides.
    Either,
}

/// The interface a launch starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interface {
    Gui,
    Tui,
}

/// Reads `--gui` and `--tui` from the arguments. With neither, `prefer_tui` (`SHEPHERD_NO_GUI=1`) asks for the
/// terminal UI, for scripts, SSH sessions and machines that have the app but are used from a terminal.
pub fn asked(args: &[String], prefer_tui: bool) -> Result<Asked, String> {
    let gui = args.iter().any(|a| a == "--gui");
    let tui = args.iter().any(|a| a == "--tui");
    match (gui, tui) {
        (true, true) => Err("--gui and --tui cannot be used together".to_owned()),
        (true, false) => Ok(Asked::Gui),
        (false, true) => Ok(Asked::Tui),
        (false, false) if prefer_tui => Ok(Asked::Tui),
        (false, false) => Ok(Asked::Either),
    }
}

/// Whether `SHEPHERD_NO_GUI` asks for the terminal UI (set, and not empty or `0`).
pub fn prefers_tui(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != "0")
}

/// The app in the first of `dirs` that has it.
pub fn find_app(dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    dirs.into_iter()
        .map(|dir| dir.join(APP_BINARY))
        .find(|path| path.is_file())
}

/// The installed app: next to this program, else on the PATH.
pub fn installed_app() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    find_app(beside.into_iter().chain(on_path))
}

/// Which interface to open. A request is kept, and fails when the app cannot open; with no request the app opens
/// when it is installed and there is a screen, and the terminal UI opens otherwise (a server without a screen, a
/// machine that only has `shepherd`) and when saved SSH machines are in use, which only the terminal UI attaches to.
pub fn choose(
    asked: Asked,
    app: Option<&Path>,
    screen: bool,
    saved_federation: bool,
) -> Result<Interface, String> {
    match asked {
        Asked::Tui => Ok(Interface::Tui),
        Asked::Gui if app.is_none() => Err(format!(
            "the desktop app is not installed ({APP_BINARY} was not found next to shepherd or on the PATH); \
             use `shepherd --tui` for the terminal UI"
        )),
        Asked::Gui if !screen => Err(
            "there is no graphical session here (no DISPLAY or WAYLAND_DISPLAY); \
             use `shepherd --tui` for the terminal UI"
                .to_owned(),
        ),
        Asked::Gui => Ok(Interface::Gui),
        Asked::Either if app.is_some() && screen && !saved_federation => Ok(Interface::Gui),
        Asked::Either => Ok(Interface::Tui),
    }
}

/// Starts the app and leaves it running on its own. It is told which `shepherd` started it, because an app opened
/// from a menu has no `~/.local/bin` on its PATH and asks that program for the sessions and the server.
pub fn open(app: &Path) -> io::Result<()> {
    let mut command = Command::new(app);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(me) = std::env::current_exe() {
        command.env("SHEPHERD_BIN", me);
    }
    crate::platform::detach_gui_command(&mut command);
    command.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_flags_say_which_interface_is_asked_and_cannot_both_be_given() {
        assert_eq!(asked(&args(&["shepherd"]), false), Ok(Asked::Either));
        assert_eq!(asked(&args(&["shepherd", "--gui"]), false), Ok(Asked::Gui));
        assert_eq!(asked(&args(&["shepherd", "--tui"]), false), Ok(Asked::Tui));
        assert!(asked(&args(&["shepherd", "--gui", "--tui"]), false).is_err());
    }

    #[test]
    fn no_gui_in_the_environment_only_decides_when_nothing_is_asked() {
        let tui = Some(std::ffi::OsStr::new("1"));
        assert!(prefers_tui(tui));
        assert!(!prefers_tui(None));
        assert!(!prefers_tui(Some(std::ffi::OsStr::new(""))));
        assert!(!prefers_tui(Some(std::ffi::OsStr::new("0"))));
        assert_eq!(asked(&args(&["shepherd"]), true), Ok(Asked::Tui));
        assert_eq!(
            asked(&args(&["shepherd", "--gui"]), true),
            Ok(Asked::Gui),
            "a request wins"
        );
    }

    #[test]
    fn a_request_is_kept_and_no_request_opens_the_app_only_when_it_can() {
        let app = Path::new("/bin/shepherd-native");
        // asking for the terminal UI always works
        assert_eq!(choose(Asked::Tui, None, false, false), Ok(Interface::Tui));
        // asking for the app fails, with the way out, when it cannot open
        let missing = choose(Asked::Gui, None, true, false).unwrap_err();
        assert!(missing.contains("not installed") && missing.contains("--tui"));
        let no_screen = choose(Asked::Gui, Some(app), false, false).unwrap_err();
        assert!(no_screen.contains("graphical") && no_screen.contains("--tui"));
        assert_eq!(
            choose(Asked::Gui, Some(app), true, false),
            Ok(Interface::Gui)
        );
        // with no request: the app when it can open, else the terminal UI
        assert_eq!(
            choose(Asked::Either, Some(app), true, false),
            Ok(Interface::Gui)
        );
        assert_eq!(choose(Asked::Either, None, true, false), Ok(Interface::Tui));
        assert_eq!(
            choose(Asked::Either, Some(app), false, false),
            Ok(Interface::Tui)
        );
        assert_eq!(
            choose(Asked::Either, Some(app), true, true),
            Ok(Interface::Tui),
            "saved SSH machines are attached to from the terminal UI"
        );
    }

    #[test]
    fn the_app_is_found_in_the_first_directory_that_has_it() {
        let root = std::env::temp_dir().join(format!("shepherd-app-find-{}", std::process::id()));
        let (empty, with_app) = (root.join("empty"), root.join("with"));
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&with_app).unwrap();
        std::fs::write(with_app.join(APP_BINARY), b"").unwrap();
        assert_eq!(
            find_app([empty.clone(), with_app.clone()]),
            Some(with_app.join(APP_BINARY))
        );
        assert_eq!(find_app([empty]), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
