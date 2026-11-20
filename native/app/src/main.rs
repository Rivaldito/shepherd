//! The native Shepherd app (milestone 1): sidebar with spaces and agents, tab bar, and the live terminal
//! surface from the server. Everything that is not drawing lives in `shepherd-app-core`.

mod commands;
mod config_panel;
mod editor;
mod graph_view;
mod grid;
mod keys;
mod logo;
mod palette;
mod panel;
mod pattern_editor;
mod prism;
mod sessions;
mod sidebar;
mod theme;
mod ui;

use iced::keyboard;
use iced::widget::canvas::Canvas;
use iced::widget::{
    button, column, container, mouse_area, row, scrollable, text, text_input, tooltip, Space,
};
use iced::{Background, Border, Color, Element, Length, Size, Subscription, Task, Theme};
use shepherd_app_core::api;
use shepherd_app_core::data::Data;
use shepherd_app_core::endpoint::{Endpoint, EndpointSender, Geometry, SNAPSHOT};
use shepherd_app_core::snapshot::Snapshot;
use shepherd_app_core::watch::StateWatcher;
use shepherd_app_core::wire::{ClientMouseButton, ClientMouseKind, Incoming, PaneSurface};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use theme::{
    item_style, mini_button_style, pad, primary_button_style, status_color, status_name, BG, DIM,
    LINE, PANEL,
};

const SIDEBAR_W: f32 = 232.0;
/// The least height of the main area: below it the area scrolls.
const MAIN_MIN_H: f32 = 600.0;
/// The height of a terminal row, which the pane headers take.
const CELL_ROW: f32 = grid::CELL_H;
const TABBAR_H: f32 = 44.0;
const STATUS_H: f32 = 28.0;
/// The top bar is 48 high in all: the 4 px prism strip and the items under it.
const TOPBAR_H: f32 = 44.0;
const STRIP_H: f32 = 4.0;
/// Margin between the terminal and what surrounds it, and the 1 px border of its frame (both sides).
const TERM_PAD: f32 = 8.0;
const TERM_FRAME: f32 = 2.0 * TERM_PAD + 2.0;
const START: Size = Size::new(1280.0, 800.0);

/// The size the window opens at: `SHEPHERD_NATIVE_SIZE=1600x1000` overrides it (for bigger demo pictures).
fn start_size() -> Size {
    std::env::var("SHEPHERD_NATIVE_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some(Size::new(w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or(START)
}

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Shepherd")
        .font(theme::FONT_FILES[0])
        .font(theme::FONT_FILES[1])
        .font(theme::FONT_FILES[2])
        .font(theme::FONT_FILES[3])
        .default_font(theme::MONO)
        .subscription(App::subscription)
        .theme(App::theme)
        .window_size(start_size())
        .run()
}

/// The session the app was switched to (see `Message::SwitchSession`); the connection thread reads it on every
/// (re)connect, so a switch is: set this, close the current connection, and let the reader reconnect.
static SOCKET_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);
/// The summary of the toast that stays while the server cannot be reached.
const LOST: &str = "Lost the connection to the server. Reconnecting…";
static MENU_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CLOSE_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static NEW_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SWITCH_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn refresh_sessions() -> Task<Message> {
    Task::perform(async { sessions::list() }, Message::Sessions)
}

fn client_socket() -> PathBuf {
    if let Some(path) = SOCKET_OVERRIDE.lock().ok().and_then(|o| o.clone()) {
        return path;
    }
    if let Some(path) = std::env::var_os("SHEPHERD_CLIENT_SOCKET_PATH") {
        return PathBuf::from(path);
    }
    if let Some(api) = std::env::var_os("SHEPHERD_SOCKET_PATH").map(PathBuf::from) {
        return sessions::client_socket_for(&api);
    }
    // nothing says where the server is: ask `shepherd status server` (a found socket is kept; a miss is
    // retried every few seconds, so the app can be opened before the server and connect once it is up)
    static FOUND: Mutex<Option<(std::time::Instant, Option<PathBuf>)>> = Mutex::new(None);
    if let Ok(mut found) = FOUND.lock() {
        let stale = match &*found {
            Some((_, Some(_))) => false,
            Some((when, None)) => when.elapsed() > Duration::from_secs(3),
            None => true,
        };
        if stale {
            *found = Some((
                std::time::Instant::now(),
                sessions::discover_client_socket(),
            ));
        }
        if let Some((_, Some(path))) = &*found {
            return path.clone();
        }
    }
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join(".config/shepherd-dev/shepherd-client.sock")
}

#[derive(Clone)]
struct Shared(Arc<Mutex<EndpointSender>>);

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Shared(sender)")
    }
}

/// Things the user can ask the server to do from the sidebar and the tab bar.
#[derive(Debug, Clone)]
pub(crate) enum Action {
    FocusSpace(String),
    CloseSpace(String, bool),
    FocusTab(String),
    CloseTab(String),
    FocusAgent(String),
    FocusPane(String),
    NewTab(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingClose {
    Workspace(String),
    LastTab {
        tab_id: String,
        workspace_id: String,
    },
}

fn focus_action_for_pane(pane: String) -> Action {
    Action::FocusPane(pane)
}

#[derive(Debug, Clone)]
enum Message {
    Connected(Shared),
    Incoming(Box<Incoming>),
    Snapshot(Box<Snapshot>),
    Lost(String),
    Key(keyboard::Event),
    Resized(Size),
    /// Next step of the prism animation.
    Tick,
    /// The sessions known to the `shepherd` binary.
    Sessions(Result<Vec<sessions::Session>, String>),
    /// Leave the current session for the one whose client socket this is.
    SwitchSession(PathBuf),
    /// The `+` of the session row: open the name box.
    NewSessionOpen,
    NewSessionInput(String),
    NewSessionSubmit,
    NewSessionCancel,
    NewSpaceOpen,
    NewSpaceFolder(String),
    NewSpaceName(String),
    NewSpaceSubmit,
    NewSpaceCancel,
    SelectWorkspace(String),
    CloseWorkspace(String),
    PickTab(String, String),
    NewTerminal(String),
    ToggleSidebar,
    /// Folds or unfolds a group of the views nav: 0 Orchestrate, 1 Inspect.
    ToggleNavGroup(u8),
    /// A started session answered (its client socket), or why it did not.
    SessionStarted(Result<PathBuf, String>),
    /// The `×` of a session: the first click warns, the second one stops it (or deletes it when it is stopped).
    CloseSession(String),
    /// What closing a session came to.
    SessionClosed(Result<String, String>),
    Act(Action),
    Acted(Result<(), String>),
    Grid(grid::Event),
    Panel(panel::PanelMsg),
    Reload,
    DataLoaded(Box<Data>),
    GoToPane(String),
    SetMode(Mode),
    StateChanged,
    TogglePalette,
    ToggleMotion,
    /// The list of notices opened from the status bar.
    ToggleHistory,
    /// Shows a view of the Shepherd panel (from whichever mode the window is in).
    GoTo(panel::Tab),
    /// Opens or closes a workspace in the sidebar.
    ToggleWorkspace(String),
    /// From the Shepherd panel: back to the terminal, on this workspace.
    ClearHistory,
    /// A pane of the terminal: open its agent in the Shepherd panel.
    OpenInShepherd(String),
    PaletteQuery(String),
    PaletteMove(isize),
    PaletteRun,
    Shoot,
    Shot(iced::window::Screenshot),
    /// One frame of the demo film: its number and the picture.
    FilmShot(u32, iced::window::Screenshot),
    ArmClose(PendingClose),
    ContextMenuOpened(ContextMenuTarget),
    ContextMenuClosed,
    /// A destructive item of the context menu: the first click arms it, the second sends `then`.
    MenuItem(&'static str, Box<Message>),
    RenameStarted(String),
    RenameInput(String),
    RenameSubmitted,
    RenameCancelled,
    MouseMoved(iced::Point),
}

/// The open command palette: what was typed and which hit is highlighted.
struct Palette {
    query: String,
    selected: usize,
}

const PALETTE_INPUT: &str = "palette-input";
const SESSION_INPUT: &str = "session-input";

/// Demo recording, a verification and promotion aid like the other `SHEPHERD_NATIVE_*` hooks: it walks
/// through [`FILM`] and saves one picture per animation tick as `frame-NNNNN.ppm` in the directory, then
/// exits. `ffmpeg -framerate 12 -i frame-%05d.ppm` turns them into a video at the real animation speed.
struct Film {
    dir: PathBuf,
    /// The animation step the film began at (it waits for the data to arrive).
    start: Option<u32>,
    saved: u32,
}

/// The scenes: what to show and for how many ticks (12 per second). The first tick of a scene only switches
/// to it; the pictures start on the next one.
const FILM: &[(Mode, panel::Tab, u32)] = &[
    (Mode::Terminal, panel::Tab::Overview, 36),
    (Mode::Shepherd, panel::Tab::Overview, 49),
    (Mode::Shepherd, panel::Tab::Runs, 37),
    (Mode::Shepherd, panel::Tab::Agents, 49),
    (Mode::Shepherd, panel::Tab::Tasks, 37),
    (Mode::Shepherd, panel::Tab::Patterns, 61),
    (Mode::Shepherd, panel::Tab::Monitor, 37),
    (Mode::Shepherd, panel::Tab::Skills, 37),
    (Mode::Shepherd, panel::Tab::Events, 49),
    (Mode::Shepherd, panel::Tab::Models, 37),
];
/// Pictures the film saves: every tick of every scene except its first.
const FILM_SHOTS: u32 = {
    let mut total = 0;
    let mut i = 0;
    while i < FILM.len() {
        total += FILM[i].2 - 1;
        i += 1;
    }
    total
};

/// What the central area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Terminal,
    Shepherd,
}

impl Mode {
    /// The other one: what `Ctrl+Shift+S` switches to.
    fn other(self) -> Mode {
        match self {
            Mode::Terminal => Mode::Shepherd,
            Mode::Shepherd => Mode::Terminal,
        }
    }
}

/// Target of a context menu (for rename).
#[derive(Debug, Clone)]
enum ContextMenuTarget {
    Tab {
        tab_id: String,
        label: String,
    },
    Workspace {
        workspace_id: String,
        label: String,
    },
    /// A session tab of the top bar: running or stopped, and whether it is the default one.
    Session {
        name: String,
        running: bool,
        default: bool,
    },
}

/// The New workspace popover: the folder, an optional name and what is wrong with the folder.
#[derive(Debug, Clone, Default)]
struct NewSpace {
    folder: String,
    name: String,
    error: Option<String>,
}

/// Active rename state.
#[derive(Debug, Clone)]
struct RenameState {
    target: ContextMenuTarget,
    input: String,
}

struct App {
    palette: Option<Palette>,
    mode: Mode,
    panel: panel::Panel,
    data: Data,
    sender: Option<Shared>,
    surface: Option<PaneSurface>,
    snapshot: Snapshot,
    status: String,
    /// The connection was lost and has not come back yet (one toast per outage).
    was_lost: bool,
    pending_close: Option<PendingClose>,
    /// Animation step of the prism canvases.
    frame: u32,
    /// Demo recording (`SHEPHERD_NATIVE_FILM=<dir>`), see [`Film`].
    film: Option<Film>,
    sessions: Vec<sessions::Session>,
    /// The name being typed for a new session, while the box is open, and what is wrong with it.
    new_session: Option<(String, Option<String>)>,
    /// The session whose `×` was clicked once, and the animation step it happened at.
    closing: Option<(String, u32)>,
    /// What the session row has to say (a warning, a result), shown in the status bar; not the connection status.
    session_note: Option<(String, Color)>,
    window: Size,
    sent: Option<(u16, u16)>,
    api_socket: PathBuf,
    context_menu: Option<(iced::Point, ContextMenuTarget)>,
    /// The folder typed in the New workspace popover, while it is open.
    new_space: Option<NewSpace>,
    /// The sidebar column is shown (Ctrl+B hides it; the main area then takes the full width).
    sidebar_visible: bool,
    /// Which groups of the views nav are folded (Orchestrate, Inspect).
    nav_collapsed: [bool; 2],
    /// The destructive item of the context menu that waits for its second click.
    menu_armed: Option<&'static str>,
    rename: Option<RenameState>,
    mouse_position: iced::Point,
    /// Whether a data load is running, and whether another was asked for meanwhile.
    reloads: shepherd_app_core::reload::Coalesce,
    /// The prism stands still (reduced motion), by choice in the status bar or `SHEPHERD_REDUCE_MOTION`.
    still: bool,
    history_open: bool,
    /// Which workspaces are open in the sidebar (the focused one is, until the user says otherwise).
    expanded: std::collections::HashMap<String, bool>,
    /// Counts every tick, even while the prism stands still: timers use it, not `frame`.
    clock: u32,
}

/// Terminal cells that fit in the window once the sidebar, tab bar and status line are taken out.
fn terminal_cells(window: Size, sidebar_width: f32) -> (u16, u16) {
    let cols = ((window.width - sidebar_width - TERM_FRAME) / grid::CELL_W)
        .floor()
        .max(20.0) as u16;
    let chrome = STRIP_H + TOPBAR_H + TABBAR_H + STATUS_H + TERM_FRAME;
    let rows = ((window.height - chrome) / grid::CELL_H).floor().max(5.0) as u16;
    (cols, rows)
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let app = Self {
            palette: std::env::var("SHEPHERD_NATIVE_PALETTE")
                .ok()
                .map(|query| Palette { query, selected: 0 }), // for pictures (make native-shot PALETTE=…)
            mode: if std::env::var_os("SHEPHERD_NATIVE_MODE").is_some_and(|m| m == "shepherd") {
                Mode::Shepherd
            } else {
                Mode::Terminal
            },
            panel: panel::Panel::default(),
            data: Data::default(),
            sender: None,
            surface: None,
            snapshot: Snapshot::default(),
            status: "connecting…".into(),
            was_lost: false,
            pending_close: None,
            window: start_size(),
            sent: None,
            api_socket: api::api_socket_for(&client_socket()),
            context_menu: None,
            menu_armed: None,
            new_space: None,
            sidebar_visible: true,
            nav_collapsed: [false, false],
            rename: None,
            mouse_position: iced::Point::ORIGIN,
            reloads: shepherd_app_core::reload::Coalesce::running(), // the first load starts below
            still: std::env::var_os("SHEPHERD_REDUCE_MOTION").is_some(),
            clock: 0,
            history_open: false,
            expanded: Default::default(),
            frame: 0,
            film: std::env::var_os("SHEPHERD_NATIVE_FILM").map(|dir| Film {
                dir: PathBuf::from(dir),
                start: None,
                saved: 0,
            }),
            sessions: Vec::new(),
            new_session: None,
            closing: None,
            session_note: None,
        };
        let folder = app.folder();
        let api = app.api_socket.clone();
        (
            app,
            Task::batch([
                Task::perform(async move { Data::load(&api, &folder) }, |d| {
                    Message::DataLoaded(Box::new(d))
                }),
                refresh_sessions(),
            ]),
        )
    }

    /// The folder the panels are about: where the focused pane is, else where the app started.
    fn folder(&self) -> PathBuf {
        self.snapshot
            .panes
            .iter()
            .find(|p| p.focused)
            .and_then(|p| p.cwd.clone())
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }

    /// Asks for a data load. While one runs, the request is kept and one more load starts when it ends.
    fn reload(&mut self) -> Task<Message> {
        if !self.reloads.request() {
            return Task::none();
        }
        self.load_now()
    }

    fn load_now(&self) -> Task<Message> {
        let folder = self.folder();
        let api = self.api_socket.clone();
        Task::perform(async move { Data::load(&api, &folder) }, |d| {
            Message::DataLoaded(Box::new(d))
        })
    }

    fn theme(&self) -> Theme {
        theme::app_theme()
    }

    fn geometry(&self) -> Geometry {
        let (cols, rows) = terminal_cells(
            self.window,
            if self.sidebar_visible { SIDEBAR_W } else { 0.0 },
        );
        Geometry {
            cols,
            rows,
            cell_width_px: grid::CELL_W.round() as u32,
            cell_height_px: grid::CELL_H.round() as u32,
        }
    }

    /// Tells the server the size of the terminal area when it changed.
    fn sync_size(&mut self) {
        let (Some(shared), geometry) = (self.sender.clone(), self.geometry()) else {
            return;
        };
        if self.sent == Some((geometry.cols, geometry.rows)) {
            return;
        }
        let resized = shared
            .0
            .lock()
            .map(|mut sender| sender.resize(geometry).is_ok())
            .unwrap_or(false);
        if resized {
            self.sent = Some((geometry.cols, geometry.rows));
        }
    }

    fn focused_pane(&self) -> Option<String> {
        self.surface
            .as_ref()?
            .panes
            .iter()
            .find(|p| p.focused)
            .map(|p| p.pane_id.clone())
    }

    fn with_sender(&mut self, send: impl FnOnce(&mut EndpointSender) -> std::io::Result<()>) {
        let Some(shared) = self.sender.clone() else {
            return;
        };
        let Ok(mut sender) = shared.0.lock() else {
            return;
        };
        if let Err(e) = send(&mut sender) {
            self.status = format!("could not send: {e}");
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Connected(sender) => {
                self.sender = Some(sender);
                self.sent = None;
                self.status = "connected".into();
                if std::mem::take(&mut self.was_lost) {
                    self.panel.drop_toasts_saying(LOST);
                    self.panel.notify(ui::info("Reconnected."));
                }
                self.sync_size();
                // verification aid: `SHEPHERD_NATIVE_FOLD=1` starts with the Inspect group folded
                if std::env::var_os("SHEPHERD_NATIVE_FOLD").is_some() {
                    self.nav_collapsed[1] = true;
                }
                // verification aid: `SHEPHERD_NATIVE_HIDESIDEBAR=1` starts with the sidebar hidden
                if std::env::var_os("SHEPHERD_NATIVE_HIDESIDEBAR").is_some() {
                    self.sidebar_visible = false;
                }
                // verification aid: `SHEPHERD_NATIVE_NEWSPACE=1` opens the New workspace popover once
                static SPACE_DONE: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if std::env::var_os("SHEPHERD_NATIVE_NEWSPACE").is_some()
                    && !SPACE_DONE.swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    self.new_space = Some(NewSpace::default());
                }
                // verification aid: `SHEPHERD_NATIVE_NEWSESSION=<name>` types that name in the `+` box and submits it
                if let Some(name) = std::env::var_os("SHEPHERD_NATIVE_NEWSESSION") {
                    if !NEW_DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        return Task::done(Message::NewSessionInput(
                            name.to_string_lossy().into_owned(),
                        ))
                        .chain(Task::done(Message::NewSessionSubmit));
                    }
                }
                // verification aid: `SHEPHERD_NATIVE_SWITCH=<client socket>` leaves for that session once connected
                if let Some(target) = std::env::var_os("SHEPHERD_NATIVE_SWITCH") {
                    if !SWITCH_DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        return Task::done(Message::SwitchSession(PathBuf::from(target)));
                    }
                }
                if std::env::var_os("SHEPHERD_NATIVE_SHOT").is_some() {
                    // verification aid: let the first frames arrive, then save the window and exit
                    return Task::batch([
                        Task::perform(
                            async { std::thread::sleep(Duration::from_millis(2500)) },
                            |_| Message::Shoot,
                        ),
                        refresh_sessions(),
                    ]);
                }
                return refresh_sessions();
            }
            Message::Lost(why) => {
                self.sender = None;
                self.status = format!("disconnected ({why}); reconnecting…");
                // one persistent error toast per outage
                if !std::mem::replace(&mut self.was_lost, true) {
                    self.panel.notify(ui::invalid(LOST));
                }
            }
            Message::Snapshot(snapshot) => {
                let before = self.folder();
                self.snapshot = *snapshot;
                if self.folder() != before {
                    return self.reload();
                }
            }
            Message::Incoming(incoming) => match *incoming {
                Incoming::Surface(surface) => self.surface = Some(*surface),
                Incoming::Patch(patch) => {
                    let mismatch = self
                        .surface
                        .as_mut()
                        .is_some_and(|surface| surface.apply(*patch).is_err());
                    if mismatch {
                        // out of step: asking for the size again makes the server send a full surface
                        self.sent = None;
                        self.sync_size();
                    }
                }
                _ => {}
            },
            Message::Key(event) => {
                if let keyboard::Event::KeyPressed { key, modifiers, .. } = &event {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.panel.disarm();
                        self.panel.clear_filter();
                        self.pending_close = None;
                    }
                    let is_p =
                        matches!(key, keyboard::Key::Character(c) if c.eq_ignore_ascii_case("p"));
                    if is_p && modifiers.control() && modifiers.shift() {
                        return Task::done(Message::TogglePalette);
                    }
                    let is_s =
                        matches!(key, keyboard::Key::Character(c) if c.eq_ignore_ascii_case("s"));
                    if is_s && modifiers.control() && modifiers.shift() {
                        return Task::done(Message::SetMode(self.mode.other()));
                    }
                    // Ctrl+B hides or shows the sidebar, from anywhere (before the terminal gets the key)
                    if modifiers.control()
                        && !modifiers.shift()
                        && matches!(key, keyboard::Key::Character(c) if c.eq_ignore_ascii_case("b"))
                    {
                        return Task::done(Message::ToggleSidebar);
                    }
                    // Tab and Shift+Tab move between the text fields of the view (iced focuses fields, not buttons)
                    if self.mode == Mode::Shepherd
                        && self.palette.is_none()
                        && matches!(key, keyboard::Key::Named(keyboard::key::Named::Tab))
                    {
                        return if modifiers.shift() {
                            iced::widget::operation::focus_previous()
                        } else {
                            iced::widget::operation::focus_next()
                        };
                    }
                    // `/` jumps to the filter of the list (a focused input has already taken the key)
                    if self.mode == Mode::Shepherd
                        && self.palette.is_none()
                        && !modifiers.control()
                        && matches!(key, keyboard::Key::Character(c) if c.as_str() == "/")
                    {
                        return iced::widget::operation::focus(ui::FILTER_ID);
                    }
                    if self.mode == Mode::Shepherd && modifiers.control() {
                        let tab = match key {
                            keyboard::Key::Character(c) => match c.as_str() {
                                "1" => Some(panel::Tab::Overview),
                                "2" => Some(panel::Tab::Runs),
                                "3" => Some(panel::Tab::Agents),
                                "4" => Some(panel::Tab::Tasks),
                                "5" => Some(panel::Tab::Patterns),
                                "6" => Some(panel::Tab::Monitor),
                                "7" => Some(panel::Tab::Skills),
                                "8" => Some(panel::Tab::Git),
                                "9" => Some(panel::Tab::Events),
                                _ => None,
                            },
                            _ => None,
                        };
                        if let Some(tab) = tab {
                            return Task::done(Message::Panel(panel::PanelMsg::Tab(tab)));
                        }
                    }
                    if self.mode == Mode::Shepherd
                        && self.panel.comm_open()
                        && self.palette.is_none()
                        && matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
                    {
                        return Task::done(Message::Panel(panel::PanelMsg::CommClose));
                    }
                    if self.new_session.is_some()
                        && matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
                    {
                        return Task::done(Message::NewSessionCancel);
                    }
                    if self.new_space.is_some()
                        && matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
                    {
                        return Task::done(Message::NewSpaceCancel);
                    }
                    // Up and Down walk the list on screen (not while a text field is being typed in)
                    if self.mode == Mode::Shepherd
                        && self.palette.is_none()
                        && !self.panel.comm_open()
                        && !modifiers.control()
                    {
                        let step = match key {
                            keyboard::Key::Named(keyboard::key::Named::ArrowDown) => 1,
                            keyboard::Key::Named(keyboard::key::Named::ArrowUp) => -1,
                            _ => 0,
                        };
                        if step != 0 {
                            if let Some(message) = ui::nav_step(step) {
                                return Task::done(message);
                            }
                        }
                    }
                    if self.palette.is_some() {
                        return match key {
                            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                                Task::done(Message::TogglePalette)
                            }
                            keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                                Task::done(Message::PaletteMove(1))
                            }
                            keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                                Task::done(Message::PaletteMove(-1))
                            }
                            _ => Task::none(),
                        };
                    }
                }
                if self.palette.is_some() || self.mode != Mode::Terminal {
                    return Task::none();
                }
                if let (
                    keyboard::Event::KeyPressed {
                        key,
                        modifiers,
                        text,
                        ..
                    },
                    Some(pane),
                ) = (&event, self.focused_pane())
                {
                    if let Some(input) = keys::map_key(key, *modifiers, text.as_deref()) {
                        self.with_sender(|s| match input {
                            keys::Input::Key(code, bits) => s.send_key(&pane, code, bits),
                            keys::Input::Text(text) => s.send_text(&pane, &text),
                        });
                    }
                }
            }
            Message::Tick => {
                self.clock = self.clock.wrapping_add(1);
                self.panel.expire_armed();
                if !self.still {
                    self.frame = self.frame.wrapping_add(1);
                }
                if let Some(task) = self.film_tick() {
                    return task;
                }
                // an unconfirmed `×` goes back to normal after ~5 s
                if let Some((_, since)) = &self.closing {
                    if self.clock.wrapping_sub(*since) > 60 {
                        self.closing = None;
                        self.session_note = None;
                    }
                }
                // sessions start and stop outside the app: look again every ~8 s while the window is open
                if self.clock.is_multiple_of(100) {
                    return refresh_sessions();
                }
            }
            Message::NewSessionOpen => {
                self.closing = None;
                self.new_session = Some((String::new(), None));
                return iced::widget::operation::focus(SESSION_INPUT);
            }
            Message::NewSessionInput(text) => self.new_session = Some((text, None)),
            Message::NewSessionCancel => self.new_session = None,
            Message::NewSpaceOpen => self.new_space = Some(NewSpace::default()),
            Message::NewSpaceFolder(text) => {
                if let Some(space) = self.new_space.as_mut() {
                    space.folder = text;
                    space.error = None;
                }
            }
            Message::NewSpaceName(text) => {
                if let Some(space) = self.new_space.as_mut() {
                    space.name = text;
                }
            }
            Message::NewSpaceCancel => self.new_space = None,
            Message::NewSpaceSubmit => {
                let Some(space) = self.new_space.clone() else {
                    return Task::none();
                };
                let folder = space.folder.trim().to_owned();
                let folder = match folder.strip_prefix('~') {
                    Some(rest) => std::env::var("HOME")
                        .map(|h| format!("{h}{rest}"))
                        .unwrap_or(folder),
                    None => folder,
                };
                // the popover stays open so the folder can be fixed
                let problem = if folder.is_empty() {
                    Some("Say which folder to open.".to_owned())
                } else if !std::path::Path::new(&folder).is_dir() {
                    Some(format!("{folder} is not a folder."))
                } else {
                    None
                };
                if let Some(problem) = problem {
                    if let Some(s) = self.new_space.as_mut() {
                        s.error = Some(problem);
                    }
                    return Task::none();
                }
                self.new_space = None;
                let name = space.name.trim().to_owned();
                let shown = if name.is_empty() {
                    std::path::Path::new(&folder)
                        .file_name()
                        .map_or(folder.clone(), |n| n.to_string_lossy().into_owned())
                } else {
                    name.clone()
                };
                let mut params = serde_json::json!({ "focus": true, "cwd": folder });
                if !name.is_empty() {
                    params["label"] = serde_json::json!(name);
                }
                self.panel
                    .notify(format!("Opened workspace {shown} in {folder}."));
                let socket = self.api_socket.clone();
                return Task::perform(
                    async move { api::call(&socket, "workspace.create", params).map(|_| ()) },
                    Message::Acted,
                );
            }
            Message::NewSessionSubmit => {
                let Some((name, _)) = self.new_session.take() else {
                    return Task::none();
                };
                let name = name.trim().to_owned();
                if let Err(why) = sessions::validate_name(&name, &self.sessions) {
                    self.new_session = Some((name, Some(why)));
                    return Task::none();
                }
                self.session_note = Some((format!("starting session `{name}`…"), DIM));
                return Task::perform(
                    async move { sessions::start(&name, Duration::from_secs(15)) },
                    Message::SessionStarted,
                );
            }
            Message::SessionStarted(Ok(socket)) => {
                return Task::batch([
                    refresh_sessions(),
                    Task::done(Message::SwitchSession(socket)),
                ]);
            }
            Message::SessionStarted(Err(why)) => {
                self.session_note =
                    Some((format!("could not start the session: {why}"), theme::BAD));
                return refresh_sessions();
            }
            Message::CloseSession(name) => {
                let Some(session) = self.sessions.iter().find(|s| s.name == name).cloned() else {
                    return Task::none();
                };
                if self.closing.as_ref().map(|(n, _)| n) != Some(&name) {
                    let current = session.client_socket() == client_socket();
                    self.session_note =
                        Some((sessions::close_hint(&session, current), theme::WARN));
                    self.closing = Some((name, self.clock));
                    return Task::none();
                }
                self.closing = None;
                let running = session.running;
                self.session_note = Some((format!("closing `{name}`…"), DIM));
                return Task::perform(
                    async move {
                        if running {
                            sessions::stop(&name).map(|_| format!("stopped `{name}`"))
                        } else {
                            sessions::delete(&name).map(|_| format!("deleted `{name}`"))
                        }
                    },
                    Message::SessionClosed,
                );
            }
            Message::SessionClosed(result) => {
                self.session_note = Some(match result {
                    Ok(done) => (done, theme::OK),
                    Err(why) => (format!("could not close the session: {why}"), theme::BAD),
                });
                return refresh_sessions();
            }
            Message::Sessions(Ok(list)) => {
                self.sessions = list;
                // verification aid: `SHEPHERD_NATIVE_MENU=session|workspace[:arm]` opens that right-click menu once
                if let Some(spec) = std::env::var_os("SHEPHERD_NATIVE_MENU") {
                    if !MENU_DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let spec = spec.to_string_lossy().into_owned();
                        let target = if spec.starts_with("session") {
                            self.sessions.first().map(|s| ContextMenuTarget::Session {
                                name: s.name.clone(),
                                running: s.running,
                                default: s.default,
                            })
                        } else {
                            self.snapshot
                                .workspaces
                                .last()
                                .map(|w| ContextMenuTarget::Workspace {
                                    workspace_id: w.workspace_id.clone(),
                                    label: w.label.clone(),
                                })
                        };
                        if let Some(target) = target {
                            self.context_menu = Some((iced::Point::new(260.0, 80.0), target));
                            if spec.ends_with(":arm") {
                                self.menu_armed = Some("close");
                            }
                        }
                    }
                }
                // verification aid: `SHEPHERD_NATIVE_CLOSE=<name>` clicks that session's `×` twice, `<name>:arm` once
                if let Some(spec) = std::env::var_os("SHEPHERD_NATIVE_CLOSE") {
                    if !CLOSE_DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let spec = spec.to_string_lossy().into_owned();
                        let (name, arm_only) = match spec.strip_suffix(":arm") {
                            Some(name) => (name.to_owned(), true),
                            None => (spec, false),
                        };
                        let first = Task::done(Message::CloseSession(name.clone()));
                        return if arm_only {
                            first
                        } else {
                            first.chain(Task::done(Message::CloseSession(name)))
                        };
                    }
                }
            }
            // without a `shepherd` binary to ask there is simply no session row
            Message::Sessions(Err(_)) => self.sessions.clear(),
            Message::SwitchSession(socket) => {
                if socket == client_socket() {
                    return Task::none();
                }
                self.closing = None;
                if let Ok(mut current) = SOCKET_OVERRIDE.lock() {
                    *current = Some(socket.clone());
                }
                self.api_socket = api::api_socket_for(&socket);
                self.panel.session_changed();
                if let Some(shared) = self.sender.take() {
                    if let Ok(sender) = shared.0.lock() {
                        // wakes the reader, which reconnects to the socket set above
                        let _ = sender.close();
                    }
                }
                self.snapshot = Snapshot::default();
                self.surface = None;
                self.data = Data::default();
                self.sent = None;
                self.status = "switching session…".into();
                return Task::batch([self.reload(), refresh_sessions()]);
            }
            Message::Resized(size) => {
                if size.width > 0.0 {
                    self.window = size;
                    self.sync_size();
                }
            }
            Message::Act(action) => {
                self.pending_close = None;
                if let Action::FocusAgent(id) = &action {
                    let pane = self
                        .snapshot
                        .agents
                        .iter()
                        .find(|a| a.display_name() == *id || a.pane_id == *id)
                        .map(|a| a.pane_id.clone())
                        .unwrap_or_default();
                    self.panel.notify(ui::info(&format!(
                        "Focused {id} (pane {pane}) in Terminal mode."
                    )));
                }
                let (method, params) = match action {
                    Action::FocusSpace(id) => {
                        ("workspace.focus", serde_json::json!({ "workspace_id": id }))
                    }
                    Action::CloseSpace(id, close_group) => (
                        "workspace.close",
                        serde_json::json!({
                            "workspace_id": id,
                            "close_group": close_group
                        }),
                    ),
                    Action::FocusTab(id) => ("tab.focus", serde_json::json!({ "tab_id": id })),
                    Action::CloseTab(id) => ("tab.close", serde_json::json!({ "tab_id": id })),
                    Action::FocusAgent(id) => ("agent.focus", serde_json::json!({ "target": id })),
                    Action::FocusPane(id) => ("pane.focus", serde_json::json!({ "pane_id": id })),
                    Action::NewTab(workspace) => (
                        "tab.create",
                        serde_json::json!({ "workspace_id": workspace, "focus": true }),
                    ),
                };
                let socket = self.api_socket.clone();
                return Task::perform(
                    async move { api::call(&socket, method, params).map(|_| ()) },
                    Message::Acted,
                );
            }
            Message::ArmClose(pending) => {
                self.pending_close = Some(pending);
                self.status = "Press close again to confirm closing the group.".into();
            }
            Message::ContextMenuOpened(target) => {
                self.menu_armed = None;
                self.context_menu = Some((self.mouse_position, target));
            }
            Message::ContextMenuClosed => {
                self.context_menu = None;
                self.menu_armed = None;
            }
            Message::MenuItem(key, then) => {
                if self.menu_armed != Some(key) {
                    self.menu_armed = Some(key);
                    return Task::none();
                }
                self.menu_armed = None;
                self.context_menu = None;
                // the menu's two clicks are the confirmation; the session close asks only once more
                if let Message::CloseSession(name) = &*then {
                    self.closing = Some((name.clone(), self.clock));
                }
                return Task::done(*then);
            }
            Message::RenameStarted(label) => {
                if let Some((_, target)) = self.context_menu.take() {
                    self.rename = Some(RenameState {
                        target,
                        input: label,
                    });
                }
            }
            Message::MouseMoved(position) => {
                self.mouse_position = position;
            }
            Message::RenameInput(value) => {
                if let Some(rename) = self.rename.as_mut() {
                    rename.input = value;
                }
            }
            Message::RenameSubmitted => {
                let Some(rename) = self.rename.take() else {
                    return Task::none();
                };
                let new_label = rename.input;
                if new_label.is_empty() {
                    return Task::none();
                }
                let (method, params) = match rename.target {
                    ContextMenuTarget::Tab { tab_id, .. } => (
                        "tab.rename",
                        serde_json::json!({ "tab_id": tab_id, "label": new_label }),
                    ),
                    ContextMenuTarget::Workspace { workspace_id, .. } => (
                        "workspace.rename",
                        serde_json::json!({ "workspace_id": workspace_id, "label": new_label }),
                    ),
                    ContextMenuTarget::Session { .. } => return Task::none(),
                };
                let socket = self.api_socket.clone();
                return Task::perform(
                    async move { api::call(&socket, method, params).map(|_| ()) },
                    Message::Acted,
                );
            }
            Message::RenameCancelled => {
                self.rename = None;
            }
            Message::Reload | Message::StateChanged => return self.reload(),
            Message::DataLoaded(data) => {
                self.data = *data;
                let again = if self.reloads.finished() {
                    self.load_now()
                } else {
                    Task::none()
                };
                let ctx = panel::Ctx {
                    api: self.api_socket.clone(),
                    data: &self.data,
                    snapshot: &self.snapshot,
                    frame: self.frame,
                };
                if self.mode == Mode::Shepherd && self.panel.comm_open() {
                    return Task::batch([
                        again,
                        self.panel.on_data(&ctx),
                        self.panel.load_feed(&ctx),
                    ]);
                }
                if self.mode == Mode::Shepherd && self.panel.tab == panel::Tab::Monitor {
                    return Task::batch([
                        again,
                        self.panel.on_data(&ctx),
                        self.panel.refresh_events(),
                    ]);
                }
                if self.mode == Mode::Shepherd && self.panel.tab == panel::Tab::Events {
                    return Task::batch([
                        again,
                        self.panel.on_data(&ctx),
                        self.panel.refresh_events(),
                    ]);
                }
                return Task::batch([again, self.panel.on_data(&ctx)]);
            }
            Message::SetMode(mode) => self.mode = mode,
            Message::GoToPane(pane) => {
                self.mode = Mode::Terminal;
                return Task::done(Message::Act(focus_action_for_pane(pane)));
            }
            Message::Panel(msg) => {
                let ctx = panel::Ctx {
                    api: self.api_socket.clone(),
                    data: &self.data,
                    snapshot: &self.snapshot,
                    frame: self.frame,
                };
                return self.panel.update(msg, &ctx);
            }
            Message::ToggleMotion => self.still = !self.still,
            Message::ToggleHistory => self.history_open = !self.history_open,
            Message::GoTo(tab) => {
                return Task::batch([
                    Task::done(Message::SetMode(Mode::Shepherd)),
                    Task::done(Message::Panel(panel::PanelMsg::Tab(tab))),
                ]);
            }
            Message::ToggleWorkspace(id) => {
                let now = self.expanded.get(&id).copied().unwrap_or_else(|| {
                    self.snapshot
                        .workspaces
                        .iter()
                        .any(|w| w.workspace_id == id && w.focused)
                });
                self.expanded.insert(id, !now);
            }
            Message::SelectWorkspace(id) => {
                // it becomes current and open; the mode and the view stay as they are
                self.expanded.insert(id.clone(), true);
                return Task::done(Message::Act(Action::FocusSpace(id)));
            }
            Message::PickTab(ws, tab) => {
                self.mode = Mode::Terminal;
                self.expanded.insert(ws, true);
                return Task::done(Message::Act(Action::FocusTab(tab)));
            }
            Message::NewTerminal(id) => {
                let name = self
                    .snapshot
                    .workspaces
                    .iter()
                    .find(|w| w.workspace_id == id)
                    .map_or(id.clone(), |w| w.label.clone());
                let tab = self.snapshot.tabs_of(&id).count() + 1;
                self.mode = Mode::Terminal;
                self.expanded.insert(id.clone(), true);
                self.panel
                    .notify(ui::info(&format!("Opened a shell in {name}, tab {tab}.")));
                return Task::done(Message::Act(Action::NewTab(id)));
            }
            Message::CloseWorkspace(id) => {
                let Some(workspace) = self
                    .snapshot
                    .workspaces
                    .iter()
                    .find(|w| w.workspace_id == id)
                else {
                    return Task::none();
                };
                let name = workspace.label.clone();
                let agents = self
                    .snapshot
                    .agents
                    .iter()
                    .filter(|a| a.workspace_id == id)
                    .count();
                let panes = self
                    .snapshot
                    .panes
                    .iter()
                    .filter(|p| p.workspace_id == id)
                    .count();
                match sidebar::close_rule(&name, agents, panes) {
                    // refused here: no server text to show under the line
                    sidebar::CloseRule::Refused(text) => {
                        self.pending_close = None;
                        self.panel.notify(ui::invalid(&text));
                    }
                    sidebar::CloseRule::Confirm(_) => {
                        let pending = PendingClose::Workspace(id.clone());
                        if self.pending_close.as_ref() == Some(&pending) {
                            self.pending_close = None;
                            self.panel.notify(format!("Closed workspace {name}."));
                            return Task::done(Message::Act(Action::CloseSpace(id, true)));
                        }
                        self.pending_close = Some(pending);
                    }
                    sidebar::CloseRule::Now => {
                        self.panel.notify(format!("Closed workspace {name}."));
                        return Task::done(Message::Act(Action::CloseSpace(id, false)));
                    }
                }
            }
            Message::ToggleNavGroup(i) => {
                if let Some(c) = self.nav_collapsed.get_mut(usize::from(i)) {
                    *c = !*c;
                }
            }
            Message::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                if !self.sidebar_visible {
                    self.panel
                        .notify("Sidebar hidden. Press Ctrl+B to bring it back.".into());
                }
                self.sent = None;
                self.sync_size();
            }
            Message::ClearHistory => {
                self.panel.clear_history();
                self.history_open = false;
            }
            Message::OpenInShepherd(agent) => {
                return Task::batch([
                    Task::done(Message::SetMode(Mode::Shepherd)),
                    Task::done(Message::Panel(panel::PanelMsg::SelectAgent(agent))),
                ]);
            }
            Message::TogglePalette => {
                if self.palette.take().is_none() {
                    self.palette = Some(Palette {
                        query: String::new(),
                        selected: 0,
                    });
                    return iced::widget::operation::focus(PALETTE_INPUT);
                }
            }
            Message::PaletteQuery(query) => {
                if let Some(palette) = self.palette.as_mut() {
                    palette.query = query;
                    palette.selected = 0;
                }
            }
            Message::PaletteMove(step) => {
                let hits = self
                    .palette
                    .as_ref()
                    .map(|p| {
                        commands::filter(&commands::build(&self.snapshot, &self.data), &p.query)
                            .len()
                    })
                    .unwrap_or(0);
                if let (Some(palette), true) = (self.palette.as_mut(), hits > 0) {
                    palette.selected =
                        (palette.selected as isize + step).rem_euclid(hits as isize) as usize;
                }
            }
            Message::PaletteRun => {
                let Some(palette) = self.palette.take() else {
                    return Task::none();
                };
                let all = commands::build(&self.snapshot, &self.data);
                let run = commands::filter(&all, &palette.query)
                    .get(palette.selected)
                    .map(|c| c.run.clone());
                return Task::batch(run.unwrap_or_default().into_iter().map(Task::done));
            }
            Message::Shoot => {
                return iced::window::latest()
                    .and_then(iced::window::screenshot)
                    .map(Message::Shot)
            }
            Message::FilmShot(index, shot) => {
                if let Some(film) = &self.film {
                    let mut out =
                        format!("P6\n{} {}\n255\n", shot.size.width, shot.size.height).into_bytes();
                    out.extend(shot.rgba.chunks(4).flat_map(|px| px[..3].iter().copied()));
                    let _ = std::fs::write(film.dir.join(format!("frame-{index:05}.ppm")), out);
                }
                if index + 1 >= FILM_SHOTS {
                    return iced::exit();
                }
            }
            Message::Shot(shot) => {
                if let Some(path) = std::env::var_os("SHEPHERD_NATIVE_SHOT") {
                    let mut out =
                        format!("P6\n{} {}\n255\n", shot.size.width, shot.size.height).into_bytes();
                    out.extend(shot.rgba.chunks(4).flat_map(|px| px[..3].iter().copied()));
                    let _ = std::fs::write(path, out);
                }
                return iced::exit();
            }
            Message::Acted(Err(e)) => self.status = e,
            Message::Acted(Ok(())) => {
                self.pending_close = None;
            }
            Message::Grid(event) => match event {
                grid::Event::Press {
                    pane,
                    focused,
                    mouse_reporting,
                    column,
                    row,
                } => {
                    if !focused {
                        return Task::done(Message::Act(Action::FocusPane(pane)));
                    }
                    if mouse_reporting {
                        self.with_sender(|s| {
                            s.send_mouse(
                                &pane,
                                ClientMouseKind::Down(ClientMouseButton::Left),
                                column,
                                row,
                                0,
                                1,
                            )
                        });
                    }
                }
                grid::Event::Release {
                    pane,
                    mouse_reporting,
                    column,
                    row,
                } => {
                    if mouse_reporting {
                        self.with_sender(|s| {
                            s.send_mouse(
                                &pane,
                                ClientMouseKind::Up(ClientMouseButton::Left),
                                column,
                                row,
                                0,
                                1,
                            )
                        });
                    }
                }
                grid::Event::Scroll {
                    pane,
                    up,
                    column,
                    row,
                } => {
                    let kind = if up {
                        ClientMouseKind::ScrollUp
                    } else {
                        ClientMouseKind::ScrollDown
                    };
                    self.with_sender(|s| s.send_mouse(&pane, kind, column, row, 0, 3));
                }
            },
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match self.mode {
            Mode::Shepherd => iced::widget::responsive(move |size| {
                let ctx = panel::Ctx {
                    api: self.api_socket.clone(),
                    data: &self.data,
                    snapshot: &self.snapshot,
                    frame: self.frame,
                };
                let body = self.panel.view(&ctx);
                // fixed sizes, scroll instead of squash: when the fixed panes and a flexible region of at
                // least 360 px do not fit, the whole main area scrolls (section 1.3 of the spec)
                let (min_w, min_h) = (self.panel.min_main_width(), MAIN_MIN_H);
                if size.width >= min_w && size.height >= min_h {
                    body
                } else {
                    theme::scroll(
                        container(body)
                            .width(Length::Fixed(size.width.max(min_w)))
                            .height(Length::Fixed(size.height.max(min_h))),
                    )
                    .direction(iced::widget::scrollable::Direction::Both {
                        vertical: iced::widget::scrollable::Scrollbar::new()
                            .width(8)
                            .scroller_width(8),
                        horizontal: iced::widget::scrollable::Scrollbar::new()
                            .width(8)
                            .scroller_width(8),
                    })
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
                }
            })
            .into(),
            Mode::Terminal => self.terminal_pane(),
        };
        let status_color_val = if self.status.starts_with("connected") {
            theme::OK
        } else if self.status.contains("disconnected")
            || self.status.starts_with("could not")
            || self.status.contains("error")
        {
            theme::BAD
        } else {
            DIM
        };
        let base = self.app_window(body, status_color_val);
        let base: Element<'_, Message> = if self.history_open {
            iced::widget::stack![base, self.history_view()].into()
        } else {
            base
        };
        let toasts = self.panel.toasts();
        let base: Element<'_, Message> = if toasts.is_empty() {
            base
        } else {
            iced::widget::stack![base, ui::toasts_view(toasts)].into()
        };
        let content: Element<'_, Message> = match &self.palette {
            Some(palette) => iced::widget::stack![base, self.palette_view(palette)].into(),
            None => base,
        };
        // Overlay: rename dialog takes precedence, then context menu
        if let Some(space) = &self.new_space {
            iced::widget::stack![content, self.new_space_view(space)].into()
        } else if self.rename.is_some() {
            iced::widget::stack![content, self.rename_view()].into()
        } else if self.context_menu.is_some() {
            iced::widget::stack![content, self.context_menu_view()].into()
        } else {
            content
        }
    }

    /// The notices of this session, newest first (at most 30), over the bottom left corner: 480 wide, 280 tall at
    /// most, opened from the status bar.
    fn history_view(&self) -> Element<'_, Message> {
        let notices = self.panel.history();
        let mut list = column![row![
            text("Notices")
                .font(theme::MONO_BOLD)
                .size(13)
                .color(theme::HI),
            container(text("")).width(Length::Fill),
            button(text("Clear").size(11.5))
                .padding([2, 8])
                .style(theme::link_button_style())
                .on_press(Message::ClearHistory),
            button(text("×").size(14))
                .padding([0, 6])
                .style(theme::close_button_style())
                .on_press(Message::ToggleHistory),
        ]
        .align_y(iced::Alignment::Center)]
        .spacing(8);
        if notices.is_empty() {
            list = list.push(
                text("Nothing yet. Every notice of this session is kept here.")
                    .size(12)
                    .color(DIM),
            );
        }
        for (time, n) in notices.iter().rev() {
            let bad = ui::is_error(n);
            let color = if bad { theme::BAD } else { theme::OK };
            list = list.push(
                row![
                    text(time.clone()).size(11).color(theme::FAINT).width(62),
                    text("■").size(8).color(color),
                    text(n.clone())
                        .size(12)
                        .color(if bad { theme::BAD } else { theme::FG }),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }
        container(
            container(theme::scroll(list).height(Length::Shrink))
                .padding(14)
                .width(480)
                .max_height(280)
                .style(theme::toast_style(theme::LINE2)),
        )
        .padding(pad(0.0, 0.0, 34.0, 12.0))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_bottom(Length::Fill)
        .into()
    }

    /// The box in the top bar that opens the command palette.
    fn search_box(&self) -> Element<'_, Message> {
        button(
            container(
                row![
                    text("⌕").size(14).color(DIM),
                    text("Jump to a view, agent or task")
                        .size(11.5)
                        .color(DIM)
                        .wrapping(iced::widget::text::Wrapping::None),
                    container(text("")).width(Length::Fill),
                    container(
                        text("Ctrl+Shift+P")
                            .size(10)
                            .color(DIM)
                            .wrapping(iced::widget::text::Wrapping::None)
                    )
                    .padding([1, 6])
                    .width(Length::Shrink)
                    .style(theme::kbd_style),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            )
            .center_y(Length::Fill),
        )
        .width(Length::Fill)
        .height(30)
        .padding([0, 10])
        .style(theme::outline_button_style(false))
        .on_press(Message::TogglePalette)
        .into()
    }

    /// What needs the human now: waiting gates and blocked agents. Orange pill; click goes to where it is decided.
    fn needs_pill(&self) -> Element<'_, Message> {
        let gates = self.data.gates.len();
        let blocked = self
            .data
            .agents
            .iter()
            .filter(|a| a.state == "blocked" || !a.notice.is_empty())
            .count();
        let n = gates + blocked;
        if n == 0 {
            return button(
                container(
                    row![
                        text("■").size(8).color(theme::OK),
                        text("all clear").size(11.5).color(DIM)
                    ]
                    .spacing(7)
                    .align_y(iced::Alignment::Center),
                )
                .center_y(Length::Fill),
            )
            .padding([0, 12])
            .height(30)
            .style(theme::link_button_style())
            .on_press(Message::GoTo(panel::Tab::Overview))
            .into();
        }
        let target = panel::Tab::Overview;
        button(
            row![
                container(
                    text(n.to_string())
                        .size(10.5)
                        .font(theme::MONO_BOLD)
                        .color(theme::BG)
                )
                .padding([0, 6])
                .style(theme::badge_style(theme::WARN)),
                text("needs you").size(12).color(theme::WARN),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .padding([5, 12])
        .style(|_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(Color {
                    a: if hot { 0.2 } else { 0.1 },
                    ..theme::WARN
                })),
                text_color: theme::WARN,
                border: Border {
                    color: theme::WARN,
                    width: 1.0,
                    radius: 14.0.into(),
                },
                ..button::Style::default()
            }
        })
        .on_press(Message::GoTo(target))
        .into()
    }

    /// One button per session (`.sess`): a dot for running or stopped, the current one lifted. Only a running
    /// session other than the current one can be switched to.
    fn session_buttons(&self) -> Element<'_, Message> {
        let current = client_socket();
        let mut buttons = row![].spacing(4);
        for session in &self.sessions {
            let here = session.client_socket() == current;
            let label = row![
                text("●")
                    .size(8)
                    .color(if session.running { theme::OK } else { DIM }),
                text(session.name.clone()).size(13),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center);
            let session_button = button(container(label).center_y(Length::Fill))
                .padding([0, 10])
                .height(30)
                .style(item_style(here));
            let session_button: Element<'_, Message> = if session.running && !here {
                session_button
                    .on_press(Message::SwitchSession(session.client_socket()))
                    .into()
            } else {
                session_button.into()
            };
            buttons = buttons.push(mouse_area(session_button).on_right_press(
                Message::ContextMenuOpened(ContextMenuTarget::Session {
                    name: session.name.clone(),
                    running: session.running,
                    default: session.default,
                }),
            ));
            // `×`: stop it (running) or delete it (stopped); the default session cannot be deleted
            if session.running || !session.default {
                let armed = self.closing.as_ref().map(|(n, _)| n) == Some(&session.name);
                let close = if armed {
                    button(
                        container(text(if session.running { "stop?" } else { "delete?" }).size(11))
                            .center(Length::Fill),
                    )
                    .padding([0, 8])
                    .height(30)
                    .style(theme::danger_button_style())
                } else {
                    button(container(text("×").size(13)).center(Length::Fill))
                        .width(24)
                        .height(30)
                        .style(theme::outline_button_style(true))
                };
                buttons = buttons.push(close.on_press(Message::CloseSession(session.name.clone())));
            }
        }
        // `+`: a new session. It opens a small name box; Enter starts it, Esc closes the box.
        match &self.new_session {
            None => {
                buttons = buttons.push(
                    button(container(text("+").size(14)).center(Length::Fill))
                        .width(30)
                        .height(30)
                        .style(theme::outline_button_style(true))
                        .on_press(Message::NewSessionOpen),
                );
            }
            Some((name, problem)) => {
                buttons = buttons.push(
                    text_input("session name", name)
                        .id(SESSION_INPUT)
                        .on_input(Message::NewSessionInput)
                        .on_submit(Message::NewSessionSubmit)
                        .style(theme::input_style)
                        .size(13)
                        .padding([5, 10])
                        .width(160),
                );
                if let Some(problem) = problem {
                    buttons = buttons.push(text(problem.clone()).size(11).color(theme::BAD));
                }
                buttons = buttons.push(
                    button(container(text("✕").size(11)).center(Length::Fill))
                        .width(30)
                        .height(30)
                        .style(theme::outline_button_style(true))
                        .on_press(Message::NewSessionCancel),
                );
            }
        }
        container(buttons.align_y(iced::Alignment::Center))
            .padding([0, 6])
            .into()
    }

    /// The two buttons that switch between the terminal and the Shepherd panel: the same place in both modes, the
    /// active one lifted. `Ctrl+Shift+S` does the same from the keyboard.
    fn mode_switch(&self) -> Element<'_, Message> {
        let segment = |glyph: &'static str, label: &'static str, mode: Mode| {
            tooltip(
                button(
                    container(
                        row![text(glyph).size(12), text(label).size(12.5)]
                            .spacing(7)
                            .align_y(iced::Alignment::Center),
                    )
                    .center_y(Length::Fill),
                )
                .padding([0, 12])
                .height(26)
                .style(item_style(self.mode == mode))
                .on_press(Message::SetMode(mode)),
                text("Switch mode: Ctrl+Shift+S").size(11),
                tooltip::Position::Bottom,
            )
        };
        container(
            row![
                segment(">_", "Terminal", Mode::Terminal),
                segment("◆", "Shepherd", Mode::Shepherd),
            ]
            .spacing(2),
        )
        .padding(2)
        .style(|_| container::Style {
            border: Border {
                color: theme::LINE2,
                width: 1.0,
                radius: 7.0.into(),
            },
            ..container::Style::default()
        })
        .into()
    }

    /// The terminal: the tabs of the workspace, and the screen in a thin frame with some margin.
    fn terminal_pane(&self) -> Element<'_, Message> {
        let screen: Element<'_, Message> = match &self.surface {
            Some(surface) => {
                let canvas = Canvas::new(grid::Grid { surface })
                    .width(Length::Fill)
                    .height(Length::Fill);
                let mut layers: Vec<Element<'_, Message>> = vec![canvas.into()];
                layers.extend(self.pane_headers(surface));
                iced::widget::stack(layers)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
            None => container(text("waiting for the server…").color(DIM))
                .center(Length::Fill)
                .into(),
        };
        column![
            self.tab_bar(),
            container(
                container(screen)
                    .padding(1)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(BG)),
                        border: Border {
                            color: theme::LINE,
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..container::Style::default()
                    }),
            )
            .padding(TERM_PAD)
            .width(Length::Fill)
            .height(Length::Fill),
        ]
        .into()
    }

    /// A strip over the top row of every pane that holds an agent: its name, kind and state, and two buttons
    /// (open it in the Shepherd panel, focus it). The row is the pane's own top border, which it covers.
    fn pane_headers<'a>(&'a self, surface: &'a PaneSurface) -> Vec<Element<'a, Message>> {
        let mut out = Vec::new();
        let several = surface.panes.len() > 1;
        for pane in &surface.panes {
            let agent = self
                .snapshot
                .agents
                .iter()
                .find(|a| a.pane_id == pane.pane_id);
            // a plain shell has no header: its first row is real content, not a border
            if agent.is_none() {
                continue;
            }
            let status = agent.map(|a| a.status());
            let blocked = status == Some(shepherd_app_core::snapshot::Status::Blocked);
            let color = status.map_or(DIM, status_color);
            let name = agent.map_or_else(|| "shell".to_owned(), |a| a.display_name());
            let word = match status {
                Some(_) if blocked => "needs you",
                Some(s) => status_name(s),
                None => "",
            };
            let place = |x: u16, y: u16| {
                pad(
                    f32::from(y) * grid::CELL_H,
                    0.0,
                    0.0,
                    f32::from(x) * grid::CELL_W,
                )
            };
            // an outline: orange when the agent needs the human, cyan on the focused pane of several
            let outline = if blocked {
                Some(Color {
                    a: 0.45,
                    ..theme::WARN
                })
            } else if several && pane.focused {
                Some(theme::ACCENT)
            } else {
                None
            };
            if let Some(color) = outline {
                out.push(
                    container(
                        container(text(""))
                            .width(f32::from(pane.rect.width) * grid::CELL_W)
                            .height(f32::from(pane.rect.height) * grid::CELL_H)
                            .style(move |_| container::Style {
                                border: Border {
                                    color,
                                    width: 1.5,
                                    radius: 0.0.into(),
                                },
                                ..container::Style::default()
                            }),
                    )
                    .padding(place(pane.rect.x, pane.rect.y))
                    .into(),
                );
            }
            let mini = |label: &'static str, msg: Message| {
                button(text(label).size(10.5))
                    .padding([0, 7])
                    .height(CELL_ROW)
                    .style(theme::link_button_style())
                    .on_press(msg)
            };
            let mut strip = row![
                text("■").size(9).color(color),
                text(name.clone())
                    .font(theme::MONO_BOLD)
                    .size(11)
                    .color(theme::HI),
                text(agent.and_then(|a| a.agent.clone()).unwrap_or_default())
                    .size(10.5)
                    .color(DIM),
                text(word).size(10.5).color(color),
                text(pane.pane_id.clone()).size(10.5).color(theme::FAINT),
                container(text("")).width(Length::Fill),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center);
            if agent.is_some() {
                strip = strip.push(mini("Open in Shepherd", Message::OpenInShepherd(name)));
            }
            strip = strip.push(mini(
                "Focus",
                Message::Act(Action::FocusPane(pane.pane_id.clone())),
            ));
            let strip = container(strip)
                .padding([0, 8])
                .width(f32::from(pane.rect.width) * grid::CELL_W)
                .height(CELL_ROW)
                .style(theme::bar_style);
            out.push(
                container(strip)
                    .padding(place(pane.rect.x, pane.rect.y))
                    .into(),
            );
        }
        out
    }

    /// `workspace › tab` of what the terminal shows now.
    fn terminal_place(&self) -> String {
        let workspace = self.snapshot.workspaces.iter().find(|w| w.focused);
        let tab = workspace.and_then(|w| {
            self.snapshot
                .tabs_of(&w.workspace_id)
                .find(|t| t.focused)
                .map(|t| t.label.clone())
        });
        match (workspace, tab) {
            (Some(w), Some(t)) => format!("{} › {t}", w.label),
            (Some(w), None) => w.label.clone(),
            _ => "no workspace".to_owned(),
        }
    }

    /// The window of both modes: prism strip, top bar with the switch, the rail, the body (the terminal or the
    /// Shepherd panel) and the status bar.
    fn app_window<'a>(
        &'a self,
        body: Element<'a, Message>,
        status_color_val: Color,
    ) -> Element<'a, Message> {
        // without the server the views keep their last data, dimmed to about 60 % (the overlay lets clicks through)
        let body: Element<'a, Message> = if self.was_lost {
            iced::widget::stack![
                body,
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color {
                            a: 0.4,
                            ..theme::BG
                        })),
                        ..container::Style::default()
                    })
            ]
            .into()
        } else {
            body
        };
        // the strip and the logo animate in both modes: they belong to the shared window
        let strip = Canvas::new(prism::Prism {
            preset: prism::Preset::STRIP,
            frame: self.frame,
        })
        .width(Length::Fill)
        .height(STRIP_H);
        let connected = self.status.starts_with("connected");
        let top_bar = container(
            row![
                button(
                    row![
                        Canvas::new(logo::Flock).width(36).height(18),
                        text("SHEPHERD")
                            .font(theme::PIXEL_BOLD)
                            .size(13)
                            .color(theme::HI),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center)
                )
                .padding(0)
                .style(theme::link_button_style())
                .on_press(Message::GoTo(panel::Tab::Overview)),
                self.mode_switch(),
                self.session_buttons(),
                // the search box gives way before anything else is cut off (up to 360 px)
                container(container(self.search_box()).max_width(360))
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Right),
                self.needs_pill(),
                text("●").size(9).color(status_color_val),
                text(if connected {
                    "connected".to_owned()
                } else if self.status.contains("disconnected")
                    || self.status.starts_with("could not")
                {
                    "disconnected".to_owned()
                } else {
                    "reconnecting".to_owned()
                })
                .size(11.5)
                .color(
                    if connected
                        || !(self.status.contains("disconnected")
                            || self.status.starts_with("could not"))
                    {
                        DIM
                    } else {
                        theme::BAD
                    }
                ),
            ]
            .spacing(14)
            .align_y(iced::Alignment::Center),
        )
        .height(TOPBAR_H)
        .align_y(iced::alignment::Vertical::Center)
        .padding([0, 14])
        .width(Length::Fill)
        .style(theme::bar_style);
        // left of the status bar: the panel's own notice, or where the terminal is and how big it is
        let left: Element<'a, Message> = match self.mode {
            Mode::Shepherd => button(self.panel.notice())
                .padding(0)
                .style(theme::link_button_style())
                .on_press(Message::ToggleHistory)
                .into(),
            Mode::Terminal => {
                let g = self.geometry();
                row![
                    text(self.terminal_place()).size(11.5).color(theme::FG),
                    text(format!("{}×{}", g.cols, g.rows)).size(11.5).color(DIM),
                ]
                .spacing(14)
                .into()
            }
        };
        let status_bar = container(
            row![
                left,
                text(
                    self.session_note
                        .as_ref()
                        .map(|(note, _)| note.clone())
                        .unwrap_or_default()
                )
                .size(11.5)
                .color(self.session_note.as_ref().map_or(DIM, |(_, color)| *color)),
                container(text("")).width(Length::Fill),
                button(
                    text(if self.still {
                        "Motion: off"
                    } else {
                        "Motion: on"
                    })
                    .size(11.5)
                    .color(DIM)
                )
                .padding(0)
                .style(theme::link_button_style())
                .on_press(Message::ToggleMotion),
                text(if connected {
                    "● connected"
                } else {
                    "○ offline"
                })
                .size(11.5)
                .color(status_color_val),
                text(self.panel.config_path().unwrap_or_default())
                    .size(11.5)
                    .color(DIM),
                text(concat!("v", env!("CARGO_PKG_VERSION")))
                    .size(11.5)
                    .color(DIM),
            ]
            .spacing(16)
            .align_y(iced::Alignment::Center),
        )
        .height(STATUS_H)
        .align_y(iced::alignment::Vertical::Center)
        .padding([0, 14])
        .width(Length::Fill)
        .style(theme::bar_style);
        container(column![
            strip,
            top_bar,
            if self.sidebar_visible {
                row![self.sidebar(), container(body).width(Length::Fill)].height(Length::Fill)
            } else {
                row![container(body).width(Length::Fill)].height(Length::Fill)
            },
            status_bar,
        ])
        .style(|_| container::Style {
            background: Some(Background::Color(PANEL)),
            ..container::Style::default()
        })
        .into()
    }

    /// The popover of the sidebar's `+`: the folder the new workspace opens in (empty: the server's folder).
    fn new_space_view<'a>(&'a self, space: &'a NewSpace) -> Element<'a, Message> {
        let field = |label: &'static str,
                     hint: &'static str,
                     value: &'a str,
                     on: fn(String) -> Message,
                     bad: bool| {
            column![
                text(label).size(11).color(DIM),
                text_input(hint, value)
                    .on_input(on)
                    .on_submit(Message::NewSpaceSubmit)
                    .style(theme::input_style_marked(bad))
                    .padding([8, 10])
                    .size(12.5),
            ]
            .spacing(4)
        };
        let mut form = column![
            text("New workspace")
                .size(13)
                .font(theme::MONO_BOLD)
                .color(theme::HI),
            text("Opens a workspace in a folder. Terminals and agents started in it work there.")
                .size(11.5)
                .color(DIM),
            field(
                "Folder",
                "~/projects/…",
                &space.folder,
                Message::NewSpaceFolder,
                space.error.is_some()
            ),
        ]
        .spacing(10);
        if let Some(error) = &space.error {
            form = form.push(text(error.clone()).size(11).color(theme::BAD));
        }
        form = form
            .push(field(
                "Name",
                "the folder name",
                &space.name,
                Message::NewSpaceName,
                false,
            ))
            .push(
                row![
                    button(container(text("Open workspace").size(13)).center_y(Length::Fill))
                        .padding([0, 12])
                        .height(30)
                        .style(theme::primary_button_style())
                        .on_press(Message::NewSpaceSubmit),
                    button(container(text("Cancel").size(13)).center_y(Length::Fill))
                        .padding([0, 12])
                        .height(30)
                        .style(theme::ghost_button_style())
                        .on_press(Message::NewSpaceCancel),
                ]
                .spacing(8),
            );
        let panel = container(form)
            .padding(14)
            .width(280)
            .style(theme::toast_style(theme::LINE2));
        let overlay = mouse_area(container(text("")).width(Length::Fill).height(Length::Fill))
            .on_press(Message::NewSpaceCancel);
        // anchored under the "+" of the sidebar header
        iced::widget::stack![
            overlay,
            container(panel).padding(pad(STRIP_H + TOPBAR_H + 16.0 + 22.0 + 4.0, 0.0, 0.0, 10.0))
        ]
        .into()
    }

    /// The context menu overlay (right-click menu): Rename, and the destructive items with the two-click rule.
    fn context_menu_view(&self) -> Element<'_, Message> {
        let Some((position, target)) = &self.context_menu else {
            return container(text("")).into();
        };
        // (label, armed label, what it sends); a plain item has no armed label
        let mut items: Vec<(String, Option<String>, Message)> = vec![];
        match target {
            ContextMenuTarget::Tab { tab_id, label } => {
                items.push(("Rename".into(), None, Message::RenameStarted(label.clone())));
                items.push((
                    "Close".into(),
                    Some(format!("Confirm: close {label}")),
                    Message::Act(Action::CloseTab(tab_id.clone())),
                ));
            }
            ContextMenuTarget::Workspace {
                workspace_id,
                label,
            } => {
                let agents = self
                    .snapshot
                    .agents
                    .iter()
                    .filter(|a| &a.workspace_id == workspace_id)
                    .count();
                items.push(("Rename".into(), None, Message::RenameStarted(label.clone())));
                items.push((
                    "New terminal".into(),
                    None,
                    Message::Act(Action::NewTab(workspace_id.clone())),
                ));
                if agents > 0 {
                    // the server would refuse anyway: say it where the click is
                    items.push((
                        "Close".into(),
                        None,
                        Message::Panel(panel::PanelMsg::Notify(ui::invalid(&format!(
                            "Workspace {label} still runs {agents} agents. Close them first, then the workspace."
                        )))),
                    ));
                } else {
                    items.push((
                        "Close".into(),
                        Some(format!("Confirm: close {label}")),
                        Message::Act(Action::CloseSpace(workspace_id.clone(), false)),
                    ));
                }
            }
            ContextMenuTarget::Session {
                name,
                running,
                default,
            } => {
                if *running {
                    // `CloseSession` asks twice itself: the second message is the confirmation
                    items.push((
                        "Close".into(),
                        Some(format!("Confirm: close {name}")),
                        Message::CloseSession(name.clone()),
                    ));
                } else if !default {
                    items.push((
                        "Delete".into(),
                        Some(format!("Confirm: delete {name}")),
                        Message::CloseSession(name.clone()),
                    ));
                } else {
                    // nothing to offer: say why instead of showing an empty menu
                    items.push((
                        "The default session cannot be deleted".into(),
                        None,
                        Message::ContextMenuClosed,
                    ));
                }
            }
        }
        let mut list = column![].spacing(0);
        for (label, armed_label, message) in items {
            let key: &'static str = match label.as_str() {
                "Close" => "close",
                _ => "delete",
            };
            let armed = armed_label.is_some() && self.menu_armed == Some(key);
            let shown = match (&armed_label, armed) {
                (Some(text), true) => text.clone(),
                _ => label.clone(),
            };
            let send = match (&armed_label, &target) {
                (Some(_), _) => Message::MenuItem(key, Box::new(message)),
                (None, _) => message,
            };
            list = list.push(
                button(
                    container(text(shown).size(12))
                        .padding([8, 16])
                        .width(Length::Fill),
                )
                .padding(0)
                .width(240.0)
                .style({
                    let destructive = armed_label.is_some();
                    move |theme, status| {
                        if armed {
                            theme::danger_button_style()(theme, status)
                        } else if destructive {
                            theme::destructive_button_style()(theme, status)
                        } else {
                            item_style(false)(theme, status)
                        }
                    }
                })
                .on_press(send),
            );
        }
        let menu_content = container(list).padding(4).style(|_| container::Style {
            background: Some(Background::Color(PANEL)),
            border: Border {
                color: Color { a: 0.5, ..LINE },
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

        // Click overlay to close menu
        let overlay = mouse_area(container(text("")).width(Length::Fill).height(Length::Fill))
            .on_press(Message::ContextMenuClosed);

        // Position the menu at the click point
        let positioned = container(menu_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding {
                top: position.y.max(4.0),
                left: position.x.max(4.0),
                right: 0.0,
                bottom: 0.0,
            });

        container(iced::widget::stack![overlay, positioned])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// The command palette, drawn over the window (not a separate window): a box to type in and the matches.
    /// The command palette: a 640 wide panel 72 px from the top over a dimmed window; rows in groups, 34 high.
    fn palette_view<'a>(&'a self, palette: &'a Palette) -> Element<'a, Message> {
        let all = commands::build(&self.snapshot, &self.data);
        let hits = commands::filter(&all, &palette.query);
        let mut list = column![].spacing(0);
        let mut group = "";
        for (index, hit) in hits.iter().take(60).enumerate() {
            if hit.group != group {
                group = hit.group;
                list = list.push(
                    container(text(group).size(11).color(DIM)).padding(pad(10.0, 14.0, 4.0, 14.0)),
                );
            }
            let dot = hit.dot;
            list = list.push(
                container(
                    row![
                        text("■").size(8).color(dot),
                        text(hit.label.clone()).size(13).width(Length::Fill),
                        text(hit.hint.clone()).size(11).color(theme::FAINT),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                )
                .width(Length::Fill)
                .height(34)
                .padding([0, 14])
                .align_y(iced::alignment::Vertical::Center)
                .style(move |_| container::Style {
                    background: (index == palette.selected)
                        .then_some(Background::Color(theme::SELECTED)),
                    ..container::Style::default()
                }),
            );
        }
        if hits.is_empty() {
            list = list
                .push(container(text("No command matches.").size(12).color(DIM)).padding([10, 14]));
        }
        let header = container(
            row![
                text("⌕").size(15).color(DIM),
                text_input("Type a view, agent, task or command", &palette.query)
                    .id(PALETTE_INPUT)
                    .on_input(Message::PaletteQuery)
                    .on_submit(Message::PaletteRun)
                    .padding(6)
                    .size(14),
                text("Esc").size(10).color(theme::FAINT),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
        )
        .height(50)
        .padding([0, 14])
        .align_y(iced::alignment::Vertical::Center)
        .style(theme::bottom_line_style);
        let footer = container(
            text("Enter to open · Up and down to move · Ctrl+Shift+P to open from anywhere")
                .size(10.5)
                .color(DIM),
        )
        .padding([8, 14])
        .width(Length::Fill)
        .style(theme::top_line_style);
        let boxed = container(column![
            header,
            theme::scroll(list).height(Length::Shrink),
            footer
        ])
        .width(640)
        .max_height(560)
        .style(|_| container::Style {
            background: Some(Background::Color(PANEL)),
            border: Border {
                color: theme::LINE2,
                width: 1.0,
                radius: 10.0.into(),
            },
            ..container::Style::default()
        });
        // the backdrop dims the whole window
        container(boxed)
            .center_x(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding {
                top: 72.0,
                ..iced::Padding::ZERO
            })
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba8(3, 3, 6, 0.72))),
                ..container::Style::default()
            })
            .into()
    }

    fn tab_bar(&self) -> Element<'_, Message> {
        // Fluid tabs: no borders, seamless blend with terminal area
        let Some(workspace) = self.snapshot.focused_workspace_id.as_deref() else {
            return container(row![].padding([0, 12]))
                .height(TABBAR_H)
                .width(Length::Fill)
                .style(theme::bar_style)
                .into();
        };

        let workspace_label = self
            .snapshot
            .workspaces
            .iter()
            .find(|w| w.workspace_id == workspace)
            .map(|w| w.label.clone())
            .unwrap_or_default();
        let mut tabs = row![container(
            text(workspace_label)
                .font(theme::MONO_BOLD)
                .size(13)
                .color(theme::HI)
        )
        .padding([0, 12])
        .center_y(Length::Fill)]
        .spacing(0);
        for tab in self.snapshot.tabs_of(workspace) {
            let label = tab.label.clone();
            let tab_id = tab.tab_id.clone();
            let tab_content = row![
                text(label.clone()).size(13),
                button(container(text("✕").size(9)).center_y(Length::Fill))
                    .padding([2, 4])
                    .width(18.0)
                    .height(18.0)
                    .style(mini_button_style())
                    .on_press(self.close_tab_message(tab)),
            ]
            .align_y(iced::Alignment::Center)
            .spacing(6);

            let tab_button = button(container(tab_content).center_y(Length::Fill))
                .padding([0, 12])
                .height(32)
                .style(item_style(tab.focused))
                .on_press(Message::Act(Action::FocusTab(tab_id.clone())));

            // Wrap with mouse_area for right-click
            let tab_with_context = mouse_area(tab_button).on_right_press(
                Message::ContextMenuOpened(ContextMenuTarget::Tab { tab_id, label }),
            );

            tabs = tabs.push(tab_with_context);
        }
        tabs = tabs.push(
            button(container(text("+").size(14)).center(Length::Fill))
                .width(30)
                .height(32)
                .style(mini_button_style())
                .on_press(Message::Act(Action::NewTab(workspace.to_owned()))),
        );

        let panes = self.surface.as_ref().map_or(0, |s| s.panes.len());
        let focused_tab = self
            .snapshot
            .tabs_of(workspace)
            .find(|t| t.focused)
            .map_or(1, |t| t.number);
        let content = row![
            crate::theme::scroll(tabs).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(0).scroller_width(0),
            )),
            container(text("")).width(Length::Fill),
            text(if self.window.width >= 1500.0 {
                format!(
                    "{panes} pane{} in tab {focused_tab}. Mouse and scrollback work as in your terminal. Ctrl+Shift+S goes to Shepherd.",
                    if panes == 1 { "" } else { "s" }
                )
            } else {
                format!("{panes} pane{} in tab {focused_tab}", if panes == 1 { "" } else { "s" })
            })
            .size(11)
            .color(DIM)
            .wrapping(iced::widget::text::Wrapping::None),
        ]
        .align_y(iced::Alignment::Center)
        .width(Length::Fill);

        container(content)
            .height(TABBAR_H)
            .align_y(iced::alignment::Vertical::Center)
            .padding([0, 10])
            .width(Length::Fill)
            .style(theme::bar_style)
            .into()
    }

    /// The sidebar (spec 4.2): the workspaces tree on top (the only part that scrolls), the views under it and the
    /// "Hide sidebar" row. The same component in both modes; only the highlights change.
    fn sidebar(&self) -> Element<'_, Message> {
        use shepherd_app_core::snapshot::Status;
        let in_shepherd = self.mode == Mode::Shepherd;
        let agent_state = |agent: &shepherd_app_core::snapshot::Agent| {
            // Shepherd's own record of an agent says more than the terminal's guess, so it wins
            self.data
                .agents
                .iter()
                .find(|a| a.id == agent.display_name())
                .map_or(agent.status(), |a| panel::agent_status(&a.state))
        };
        let needs_human = |agent: &shepherd_app_core::snapshot::Agent| {
            self.data
                .agents
                .iter()
                .any(|a| a.id == agent.display_name() && !a.notice.is_empty())
        };
        let header = row![
            container(text("Workspaces").font(theme::PIXEL).size(10).color(DIM))
                .width(Length::Fill),
            button(container(text("+").size(14).color(DIM)).center(Length::Fill))
                .padding(0)
                .width(22)
                .height(22)
                .style(theme::ghost_button_style())
                .on_press(Message::NewSpaceOpen),
        ]
        .height(22)
        .padding([0, 8])
        .align_y(iced::Alignment::Center);

        let mut blocks = column![].spacing(4).width(Length::Fill);
        if self.snapshot.workspaces.is_empty() {
            blocks = blocks.push(
                container(
                    text("No workspace open. Use + to open a folder.")
                        .size(11)
                        .color(DIM),
                )
                .padding([4, 8]),
            );
        }
        for workspace in &self.snapshot.workspaces {
            let id = workspace.workspace_id.clone();
            let mine: Vec<&shepherd_app_core::snapshot::Agent> = self
                .snapshot
                .agents
                .iter()
                .filter(|a| a.workspace_id == id)
                .collect();
            let panes = self
                .snapshot
                .panes
                .iter()
                .filter(|p| p.workspace_id == id)
                .count();
            let shells = panes.saturating_sub(mine.len());
            let panes_of_agents: Vec<sidebar::AgentPane> = mine
                .iter()
                .map(|a| sidebar::AgentPane {
                    status: agent_state(a),
                    needs_human: needs_human(a),
                })
                .collect();
            let roll = sidebar::rollup(&panes_of_agents, shells);
            let current = workspace.focused;
            let open = self.expanded.get(&id).copied().unwrap_or(current);
            let name =
                sidebar::ellipsize(&workspace.label, sidebar::ws_name_max_chars(&roll.label));
            let armed = matches!(
                &self.pending_close,
                Some(PendingClose::Workspace(w)) if *w == id
            );
            let chevron = button(
                container(
                    text(if open { "⌄" } else { "›" })
                        .size(13)
                        .color(theme::FAINT),
                )
                .center(Length::Fill),
            )
            .padding(0)
            .width(22)
            .height(30)
            .style(theme::ghost_button_style())
            .on_press(Message::ToggleWorkspace(id.clone()));
            let ws_row: Element<'_, Message> = if armed {
                // the armed close grows leftwards over the name and the roll-up label
                let confirm = match sidebar::close_rule(&workspace.label, mine.len(), panes) {
                    sidebar::CloseRule::Confirm(label) => label,
                    _ => "Confirm: close".to_owned(),
                };
                row![
                    chevron,
                    button(
                        container(text(confirm).size(11).font(theme::MONO_BOLD))
                            .center(Length::Fill)
                    )
                    .padding(0)
                    .width(Length::Fill)
                    .height(24)
                    .style(theme::danger_button_style())
                    .on_press(Message::CloseWorkspace(id.clone())),
                    Space::new().width(3),
                ]
                .align_y(iced::Alignment::Center)
                .into()
            } else {
                let name_btn = button(
                    container(
                        row![
                            container(text("")).width(7).height(7).style({
                                let c = roll.square;
                                move |_| container::Style {
                                    background: Some(Background::Color(c)),
                                    border: Border {
                                        radius: 1.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }
                            }),
                            text(name).size(12.5).color(if current {
                                theme::HI
                            } else {
                                theme::TEXT2
                            }),
                            container(text("")).width(Length::Fill),
                            text(roll.label.clone()).size(10.5).color(roll.label_color),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    )
                    .center_y(Length::Fill),
                )
                .padding([0, 4])
                .width(Length::Fill)
                .height(30)
                .style(theme::ghost_button_style())
                .on_press(Message::SelectWorkspace(id.clone()));
                let name_btn = mouse_area(name_btn).on_right_press(Message::ContextMenuOpened(
                    ContextMenuTarget::Workspace {
                        workspace_id: id.clone(),
                        label: workspace.label.clone(),
                    },
                ));
                row![
                    chevron,
                    name_btn,
                    button(container(text("×").size(11).color(theme::FAINT)).center(Length::Fill))
                        .padding(0)
                        .width(24)
                        .height(24)
                        .style(theme::close_button_style())
                        .on_press(Message::CloseWorkspace(id.clone())),
                    Space::new().width(3),
                ]
                .align_y(iced::Alignment::Center)
                .into()
            };
            let ws_row = container(ws_row)
                .height(sidebar::WS_ROW_H)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: current.then_some(Background::Color(theme::PANEL)),
                    border: Border {
                        color: if current {
                            theme::LINE2
                        } else {
                            Color::TRANSPARENT
                        },
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..container::Style::default()
                });
            let mut block = column![ws_row].width(Length::Fill);
            if open {
                // the tabs row: the only row of the sidebar that wraps
                let mut chips = row![text("tabs").size(10.5).color(theme::FAINT)]
                    .spacing(4)
                    .align_y(iced::Alignment::Center)
                    .width(Length::Fill);
                let tabs: Vec<_> = self.snapshot.tabs_of(&id).collect();
                if tabs.is_empty() {
                    chips = chips.push(text("none yet").size(11).color(theme::FAINT));
                }
                for tab in &tabs {
                    let shown = !in_shepherd && current && tab.focused;
                    let count = self
                        .snapshot
                        .panes
                        .iter()
                        .filter(|p| p.tab_id == tab.tab_id)
                        .count();
                    chips = chips.push(
                        button(
                            container(
                                row![
                                    text(sidebar::ellipsize(&tab.label, 14))
                                        .size(11)
                                        .color(if shown { theme::HI } else { theme::TEXT2 }),
                                    text(count.to_string()).size(11).color(theme::FAINT),
                                ]
                                .spacing(4),
                            )
                            .center_y(Length::Fill),
                        )
                        .padding([0, 7])
                        .height(22)
                        .style(move |_, status| {
                            let hot =
                                matches!(status, button::Status::Hovered | button::Status::Pressed);
                            button::Style {
                                background: if shown {
                                    Some(Background::Color(theme::SELECTED))
                                } else {
                                    hot.then_some(Background::Color(theme::HOVER))
                                },
                                text_color: theme::TEXT2,
                                border: Border {
                                    color: if shown { theme::LINE3 } else { theme::LINE2 },
                                    width: 1.0,
                                    radius: 4.0.into(),
                                },
                                ..button::Style::default()
                            }
                        })
                        .on_press(Message::PickTab(id.clone(), tab.tab_id.clone())),
                    );
                }
                let mut inner = column![container(chips.wrap()).padding(pad(0.0, 0.0, 6.0, 0.0))]
                    .spacing(0)
                    .width(Length::Fill);
                // one row per pane that runs an agent, in tab order and then pane order
                let mut agent_rows = 0;
                for tab in &tabs {
                    for agent in mine.iter().filter(|a| a.tab_id == tab.tab_id) {
                        agent_rows += 1;
                        let status = agent_state(agent);
                        let flagged = needs_human(agent);
                        let urgent = status == Status::Blocked || flagged;
                        let color = if urgent {
                            theme::WARN
                        } else {
                            status_color(status)
                        };
                        let glyph = if urgent {
                            "!"
                        } else {
                            theme::status_glyph(status)
                        };
                        let word = if urgent && flagged {
                            "needs you".to_owned()
                        } else {
                            status_name(status).to_owned()
                        };
                        let focused = !in_shepherd && agent.focused;
                        let icon = container(text(glyph).size(9).font(theme::MONO_BOLD).color(BG))
                            .width(15)
                            .height(15)
                            .center(15)
                            .style(move |_| container::Style {
                                background: Some(Background::Color(color)),
                                border: Border {
                                    radius: 3.0.into(),
                                    ..Border::default()
                                },
                                ..container::Style::default()
                            });
                        let target = if in_shepherd {
                            Message::GoToPane(agent.pane_id.clone())
                        } else {
                            Message::Act(Action::FocusPane(agent.pane_id.clone()))
                        };
                        inner = inner.push(
                            button(
                                container(
                                    row![
                                        icon,
                                        text(sidebar::ellipsize(
                                            &agent.display_name(),
                                            sidebar::agent_name_max_chars(&word)
                                        ))
                                        .size(12.5)
                                        .color(theme::FG),
                                        container(text("")).width(Length::Fill),
                                        text(word).size(10.5).color(color),
                                    ]
                                    .spacing(8)
                                    .align_y(iced::Alignment::Center),
                                )
                                .center_y(Length::Fill),
                            )
                            .padding([0, 6])
                            .width(Length::Fill)
                            .height(sidebar::AGENT_ROW_H)
                            .style(move |_, status| {
                                let hot = matches!(
                                    status,
                                    button::Status::Hovered | button::Status::Pressed
                                );
                                button::Style {
                                    background: if focused {
                                        Some(Background::Color(theme::SURFACE))
                                    } else {
                                        hot.then_some(Background::Color(theme::HOVER))
                                    },
                                    text_color: theme::FG,
                                    border: Border {
                                        color: if focused {
                                            Color {
                                                a: 0.55,
                                                ..theme::ACCENT
                                            }
                                        } else {
                                            Color::TRANSPARENT
                                        },
                                        width: 1.0,
                                        radius: 5.0.into(),
                                    },
                                    ..button::Style::default()
                                }
                            })
                            .on_press(target),
                        );
                    }
                }
                if let Some(line) = sidebar::shells_line(shells, agent_rows) {
                    inner = inner.push(
                        container(text(line).size(10.5).color(theme::FAINT))
                            .padding(pad(3.0, 6.0, 0.0, 6.0)),
                    );
                }
                if panes == 0 {
                    inner = inner.push(
                        container(
                            text("No terminals yet. Open one to start a shell or an agent here.")
                                .size(11)
                                .color(DIM),
                        )
                        .padding(pad(2.0, 6.0, 4.0, 6.0)),
                    );
                }
                inner = inner.push(
                    container(
                        button(
                            container(
                                row![
                                    text("›_").size(11).color(DIM),
                                    text("New terminal").size(11.5).color(DIM),
                                ]
                                .spacing(8)
                                .align_y(iced::Alignment::Center),
                            )
                            .center_y(Length::Fill),
                        )
                        .padding([0, 6])
                        .width(Length::Fill)
                        .height(26)
                        .style(theme::ghost_button_style())
                        .on_press(Message::NewTerminal(id.clone())),
                    )
                    .padding(pad(4.0, 0.0, 0.0, 0.0)),
                );
                // indent 10, a 1 px guide, then the content (191 wide)
                block = block.push(
                    row![
                        Space::new().width(10),
                        container(text(""))
                            .width(1)
                            .height(Length::Fill)
                            .style(|_| container::Style {
                                background: Some(Background::Color(theme::LINE)),
                                ..container::Style::default()
                            }),
                        Space::new().width(10),
                        container(inner).width(Length::Fill),
                    ]
                    .padding(pad(6.0, 0.0, 4.0, 0.0)),
                );
            }
            blocks = blocks.push(block);
        }

        // only this list scrolls, and only vertically; in a very short window the whole column does
        let short = self.window.height < sidebar::SHORT_WINDOW;
        let list: Element<'_, Message> = if short {
            blocks.into()
        } else {
            theme::scroll(blocks).height(Length::Fill).into()
        };
        let workspaces = column![header, Space::new().height(8), list].height(if short {
            Length::Shrink
        } else {
            Length::Fill
        });

        // the views: the same in both modes; no item is active in the Terminal mode
        let ctx = panel::Ctx {
            api: self.api_socket.clone(),
            data: &self.data,
            snapshot: &self.snapshot,
            frame: self.frame,
        };
        let nav = container(self.panel.nav(&ctx, in_shepherd, self.nav_collapsed))
            .height(sidebar::nav_height(
                !self.nav_collapsed[0],
                !self.nav_collapsed[1],
            ))
            .width(Length::Fill);
        let hide = button(
            container(
                row![
                    // a small panel icon: a rounded box with a line at one third
                    container(
                        row![
                            Space::new().width(3),
                            container(text(""))
                                .width(1)
                                .height(Length::Fill)
                                .style(|_| container::Style {
                                    background: Some(Background::Color(theme::FAINT)),
                                    ..container::Style::default()
                                }),
                        ]
                        .height(Length::Fill)
                    )
                    .width(12)
                    .height(12)
                    .style(|_| container::Style {
                        border: Border {
                            color: theme::FAINT,
                            width: 1.0,
                            radius: 2.0.into(),
                        },
                        ..container::Style::default()
                    }),
                    text("Hide sidebar").size(11.5).color(DIM),
                    container(text("")).width(Length::Fill),
                    text("Ctrl+B").size(10).color(theme::FAINT),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            )
            .center_y(Length::Fill),
        )
        .padding([0, 8])
        .width(Length::Fill)
        .height(28)
        .style(theme::ghost_button_style())
        .on_press(Message::ToggleSidebar);
        let body = column![workspaces, nav, hide]
            .spacing(20)
            .padding(pad(16.0, 10.0, 14.0, 10.0));
        let body: Element<'_, Message> = if short {
            theme::scroll(body).into()
        } else {
            body.into()
        };
        row![
            container(body)
                .width(sidebar::WIDTH - 1.0)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(BG)),
                    ..container::Style::default()
                }),
            container(text(""))
                .width(1)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(theme::LINE)),
                    ..container::Style::default()
                }),
        ]
        .width(sidebar::WIDTH)
        .height(Length::Fill)
        .into()
    }

    /// The rename dialog overlay.
    fn rename_view(&self) -> Element<'_, Message> {
        let Some(rename) = &self.rename else {
            return container(text("")).into();
        };
        let title = match &rename.target {
            ContextMenuTarget::Tab { .. } => "Rename Tab",
            ContextMenuTarget::Workspace { .. } => "Rename Workspace",
            ContextMenuTarget::Session { .. } => "Rename Session",
        };

        let input = text_input("New name", &rename.input)
            .on_input(Message::RenameInput)
            .on_submit(Message::RenameSubmitted)
            .padding(8)
            .size(13);

        let submit_btn = button(text("Rename").size(12))
            .padding([6, 16])
            .style(primary_button_style())
            .on_press(Message::RenameSubmitted);

        let cancel_btn = button(text("Cancel").size(12))
            .padding([6, 16])
            .style(mini_button_style())
            .on_press(Message::RenameCancelled);

        let dialog = container(
            column![
                container(text(title).size(14)).padding(pad(12.0, 16.0, 8.0, 16.0)),
                input,
                row![cancel_btn, submit_btn]
                    .spacing(8)
                    .padding(pad(8.0, 16.0, 12.0, 16.0)),
            ]
            .spacing(0),
        )
        .width(320)
        .padding(0)
        .style(|_| container::Style {
            background: Some(Background::Color(PANEL)),
            border: Border {
                color: Color { a: 0.5, ..LINE },
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

        // Dark overlay background
        let overlay = container(text(""))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(Color {
                    a: 0.4,
                    ..Color::BLACK
                })),
                ..container::Style::default()
            });

        container(iced::widget::stack![
            overlay,
            container(dialog).center(Length::Fill)
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn workspace_has_linked_children(&self, workspace_id: &str) -> bool {
        let Some(workspace) = self
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == workspace_id)
        else {
            return false;
        };
        let Some(worktree) = workspace
            .worktree
            .as_ref()
            .filter(|worktree| !worktree.is_linked_worktree)
        else {
            return false;
        };
        self.snapshot.workspaces.iter().any(|candidate| {
            candidate.workspace_id != workspace_id
                && candidate.worktree.as_ref().is_some_and(|candidate| {
                    candidate.key == worktree.key && candidate.is_linked_worktree
                })
        })
    }

    fn close_tab_message(&self, tab: &shepherd_app_core::snapshot::Tab) -> Message {
        let workspace_id = tab.workspace_id.clone();
        let last_tab = self.snapshot.tabs_of(&workspace_id).count() <= 1;
        if last_tab && self.workspace_has_linked_children(&workspace_id) {
            let pending = PendingClose::LastTab {
                tab_id: tab.tab_id.clone(),
                workspace_id: workspace_id.clone(),
            };
            if self.pending_close.as_ref() == Some(&pending) {
                Message::Act(Action::CloseSpace(workspace_id, true))
            } else {
                Message::ArmClose(pending)
            }
        } else {
            Message::Act(Action::CloseTab(tab.tab_id.clone()))
        }
    }

    /// The next step of the demo film, if one is being recorded.
    fn film_tick(&mut self) -> Option<Task<Message>> {
        let film = self.film.as_mut()?;
        if self.sender.is_none() || self.frame < 40 {
            return None;
        }
        let elapsed = self.frame - *film.start.get_or_insert(self.frame);
        let mut from = 0;
        for (mode, tab, len) in FILM {
            if elapsed < from + len {
                if elapsed == from {
                    return Some(Task::batch([
                        Task::done(Message::SetMode(*mode)),
                        Task::done(Message::Panel(panel::PanelMsg::Tab(*tab))),
                    ]));
                }
                let index = film.saved;
                film.saved += 1;
                return Some(
                    iced::window::latest()
                        .and_then(iced::window::screenshot)
                        .map(move |shot| Message::FilmShot(index, shot)),
                );
            }
            from += len;
        }
        None
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            // ~12 fps for the prism strip and logo, which both modes show in the shared top bar
            Subscription::run(ticks),
            Subscription::run(connection),
            Subscription::run_with(self.folder(), watch_files),
            keyboard::listen().map(Message::Key),
            iced::window::resize_events().map(|(_, size)| Message::Resized(size)),
            iced::event::listen_with(|event, _status, _window_id| match event {
                iced::event::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::MouseMoved(position))
                }
                _ => None,
            }),
        ])
    }
}

/// Tells the app when a file the panels read changes (agents, runs, tasks, bus), so they never poll.
/// One `Tick` every 83 ms (iced's own timer needs an async runtime feature this app does not enable).
fn ticks() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(1, async move |mut output| {
        use iced::futures::{channel::mpsc, SinkExt, StreamExt};
        let (tx, mut rx) = mpsc::unbounded::<()>();
        std::thread::spawn(move || {
            // ends by itself once the subscription is dropped and nobody receives
            while tx.unbounded_send(()).is_ok() {
                std::thread::sleep(Duration::from_millis(83));
            }
        });
        while rx.next().await.is_some() {
            if output.send(Message::Tick).await.is_err() {
                break;
            }
        }
    })
}

#[allow(clippy::ptr_arg)] // iced hands the subscription data over as &PathBuf
fn watch_files(folder: &PathBuf) -> impl iced::futures::Stream<Item = Message> {
    let folder = folder.clone();
    iced::stream::channel(16, async move |mut output| {
        use iced::futures::{channel::mpsc, SinkExt, StreamExt};
        let (tx, mut rx) = mpsc::unbounded::<()>();
        // agents write their files in bursts: wait for a quiet moment (at most a second) and report it once
        let (raw_tx, raw_rx) = std::sync::mpsc::channel::<()>();
        let (calm_tx, calm_rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            shepherd_app_core::reload::debounce(
                &raw_rx,
                &calm_tx,
                Duration::from_millis(200),
                Duration::from_secs(1),
            )
        });
        std::thread::spawn(move || {
            while calm_rx.recv().is_ok() {
                if tx.unbounded_send(()).is_err() {
                    break;
                }
            }
        });
        let watcher = StateWatcher::start(move || {
            let _ = raw_tx.send(());
        });
        let _keep_alive = match watcher {
            Ok(mut watcher) => {
                let _ = watcher.watch(
                    &shepherd_app_core::shepherd_core::config::state_dir(),
                    false,
                );
                let _ = watcher.watch(
                    &shepherd_app_core::data::project_root_at(&folder).join(".shepherd"),
                    true,
                );
                Some(watcher)
            }
            Err(_) => None,
        };
        while rx.next().await.is_some() {
            if output.send(Message::StateChanged).await.is_err() {
                break;
            }
        }
    })
}

/// Reads the server on its own thread (the protocol is blocking), reconnecting when it goes away, and
/// forwards what arrives.
fn connection() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(256, async |mut output| {
        use iced::futures::{channel::mpsc, SinkExt, StreamExt};
        let (tx, mut rx) = mpsc::unbounded::<Message>();
        std::thread::spawn(move || loop {
            let geometry = Geometry {
                cols: 120,
                rows: 36,
                cell_width_px: grid::CELL_W.round() as u32,
                cell_height_px: grid::CELL_H.round() as u32,
            };
            match Endpoint::connect(&client_socket(), geometry) {
                Err(e) => {
                    let _ = tx.unbounded_send(Message::Lost(e.to_string()));
                }
                Ok(mut endpoint) => {
                    if let Ok(sender) = endpoint.sender() {
                        let _ = tx.unbounded_send(Message::Connected(Shared(Arc::new(
                            Mutex::new(sender),
                        ))));
                    }
                    loop {
                        let message = match endpoint.read() {
                            Ok(Incoming::Control { kind, data }) if kind == SNAPSHOT => {
                                match Snapshot::parse(&data) {
                                    Ok(snapshot) => Message::Snapshot(Box::new(snapshot)),
                                    Err(_) => continue,
                                }
                            }
                            Ok(incoming) => Message::Incoming(Box::new(incoming)),
                            Err(e) => {
                                let _ = tx.unbounded_send(Message::Lost(e.to_string()));
                                break;
                            }
                        };
                        if tx.unbounded_send(message).is_err() {
                            return;
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        });
        while let Some(message) = rx.next().await {
            if output.send(message).await.is_err() {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_shortcut_toggles_between_the_two_modes() {
        assert_eq!(Mode::Terminal.other(), Mode::Shepherd);
        assert_eq!(Mode::Shepherd.other(), Mode::Terminal);
        assert_eq!(Mode::Terminal.other().other(), Mode::Terminal);
    }

    use super::*;

    #[test]
    fn the_terminal_area_is_what_the_window_leaves() {
        assert_eq!(
            terminal_cells(Size::new(1280.0, 800.0), SIDEBAR_W),
            (132, 38)
        );
        assert_eq!(
            terminal_cells(Size::new(100.0, 100.0), SIDEBAR_W),
            (20, 5),
            "never smaller than a usable minimum"
        );
    }

    #[test]
    fn go_to_terminal_focuses_the_pane_not_the_agent() {
        assert!(matches!(
            focus_action_for_pane("pane-1".into()),
            Action::FocusPane(id) if id == "pane-1"
        ));
    }
}
