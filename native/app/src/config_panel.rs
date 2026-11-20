//! Skills, Git, Events and Models: the tabs about configuration and the world around the agents.
//! Loading happens off the UI thread (git and the bus can be slow); the only writes are the model forms.

use crate::panel::{detail_box, list_box, PanelMsg, Tab};
use crate::theme::{card_style, item_style, ACCENT, DIM, OK, WARN};
use crate::Message;
use iced::widget::{button, checkbox, column, container, row, text, text_editor, text_input};
use iced::{Background, Color, Element, Length, Task};
use shepherd_app_core::actions;
use shepherd_app_core::config_data::{self, Models, SkillRow, TopicRow};
use shepherd_app_core::installed::{self, InstalledRow};
use shepherd_app_core::shepherd_core::bus::Message as BusMessage;
use shepherd_app_core::shepherd_core::config::{Role, Vendor};
use shepherd_app_core::shepherd_core::git::GitRepoStatus;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum CfgMsg {
    Skills(Result<Vec<SkillRow>, String>),
    Git(Result<GitRepoStatus, String>),
    Topics(Result<Vec<TopicRow>, String>),
    Feed(Result<Vec<BusMessage>, String>),
    Models(Result<Models, String>),
    /// What this computer has: agent CLIs, their vendors and models.
    Installed(Vec<InstalledRow>),
    /// The models that the agents list themselves (these arrive after `Installed`).
    InstalledLive(std::collections::HashMap<String, Vec<String>>),
    SelectInstalled,
    /// Start a vendor for an agent that is installed but has none, filled in from what was found.
    AddVendorFrom(String),
    // The window that assigns skills to a role
    SkillPickOpen,
    SkillPickToggle(String),
    SkillPickFilter(String),
    SkillPickDone,
    SkillPickCancel,
    SelectSkill(String),
    // Skill editor
    SkillNewOpen,
    SkillNewName(String),
    SkillNewRepo(bool),
    SkillNewSubmit,
    SkillNewCancel,
    SkillCustomize(String),
    SkillEditOpen(String),
    SkillEdit(text_editor::Action),
    SkillSave,
    SkillCancel,
    /// The first click on Delete warns, the second one deletes.
    SkillDelete(String),
    /// Make a skill global (every agent gets it) or take it out of the global ones.
    SkillGlobal(String, bool),
    SelectWorktree(String),
    /// Open the new-branch (`false`) or new-worktree (`true`) form.
    GitFormOpen(bool),
    GitFormFirst(String),
    GitFormSecond(String),
    GitFormSubmit,
    GitFormCancel,
    /// The first click on Remove warns, the second one removes the worktree.
    WorktreeRemove(String),
    GitDone(Result<String, String>),
    TopicFormOpen(bool),
    TopicFormFirst(String),
    TopicFormSecond(String),
    TopicFormSubmit,
    TopicFormCancel,
    TopicDone(Result<String, String>),
    OpenAgent(String),
    SelectTopic(String),
    /// Events: show only the messages of this partition.
    MsgPartition(Option<u32>),
    /// The box at the foot of Events: what is typed, and sending it to the open topic.
    ComposerText(String),
    ComposerPartition(Option<u32>),
    LoadOlder,
    ComposerSend,
    SelectVendor(String),
    SelectRole(String),
    VendorField(VField, String),
    VendorMcp(bool),
    VendorDisabled(bool),
    RoleField(RField, String),
    RoleWorktree(bool),
    RoleSpawn(bool),
    NewVendor,
    NewRole,
    SelectRoles,
    RolesVendor(String, String),
    RolesModel(String, String),
    RolesRemove(String),
    Discard,
    LookAgain,
    Save,
    Delete,
    Refresh,
    RefreshEvents,
    RefreshSkills,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VField {
    Name,
    Command,
    Args,
    Models,
    DefaultModel,
    ModelFlag,
    ShepherdAgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RField {
    Name,
    Vendor,
    Model,
    Fallback,
    MustDiffer,
    Skills,
    Prompt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Editing {
    Vendor(String),
    Role(String),
    /// The table of every role: vendor and model of each, edited together and saved once.
    Roles,
    /// The list of what is installed on this computer.
    Installed,
}

/// A skill's `SKILL.md` being edited.
pub struct SkillEdit {
    name: String,
    file: PathBuf,
    content: text_editor::Content,
    /// The text as it is on disk.
    saved: String,
    /// What Shepherd's validation found in the last save.
    problems: Vec<String>,
}

impl SkillEdit {
    fn open(name: &str, file: PathBuf) -> Result<Self, String> {
        let saved =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        Ok(Self {
            name: name.to_owned(),
            content: text_editor::Content::with_text(&saved),
            saved,
            file,
            problems: Vec::new(),
        })
    }

    fn dirty(&self) -> bool {
        self.content.text() != self.saved
    }
}

/// The skills ticked in the window that assigns them to a role.
pub struct SkillPicker {
    selected: Vec<String>,
    filter: String,
}

/// The name and place of a skill about to be created.
pub struct SkillNew {
    name: String,
    /// In the project (`.shepherd/skills`) instead of for the user.
    repo: bool,
    error: Option<String>,
}

#[derive(Default)]
pub struct ConfigPanel {
    /// Refresh was pressed on Events / Skills: the next answer says how many it found.
    announce_topics: bool,
    announce_skills: bool,
    /// The form as it was when it was opened or last saved; the save bar shows while it differs.
    baseline: String,
    /// Roles changed in the table and not saved yet.
    role_pending: std::collections::BTreeMap<String, Role>,
    /// Roles removed in the table and not saved yet.
    role_removed: std::collections::BTreeSet<String>,
    /// `Look again` was pressed: the next detection says how many agent commands it found.
    looking_again: bool,
    /// What the last action was doing, shown above the server's refusal.
    fail: &'static str,
    installed: Vec<InstalledRow>,
    /// The window that assigns skills to the role being edited, while it is open.
    skill_picker: Option<SkillPicker>,
    /// The skill open in the editor.
    skill_edit: Option<SkillEdit>,
    /// The new-skill form, while it is open.
    skill_new: Option<SkillNew>,
    /// The skill whose Delete was clicked once.
    armed_skill: Option<String>,
    /// The API socket of the session shown; topics and messages are asked of it.
    api: PathBuf,
    skills: Vec<SkillRow>,
    skill: Option<String>,
    git: Option<GitRepoStatus>,
    worktree: Option<String>,
    /// The new-branch or new-worktree form, while it is open.
    git_form: Option<GitForm>,
    topic_form: Option<TopicForm>,
    /// The worktree whose Remove was clicked once.
    armed_worktree: Option<String>,
    topics: Vec<TopicRow>,
    topic: Option<String>,
    msg_partition: Option<u32>,
    /// Where the composer sends: a partition, or by key (None).
    composer_partition: Option<u32>,
    /// How many messages of the open topic are loaded: 50 more every time `Load older` is pressed (at most 500).
    feed_limit: usize,
    composer: String,
    feed: Vec<BusMessage>,
    models: Models,
    editing: Option<Editing>,
    vendor: Vendor,
    role: Role,
    /// Boxes of the model forms (a list or a half-typed word is not a valid value yet).
    vfields: Vec<(VField, String)>,
    rfields: Vec<(RField, String)>,
    is_new: bool,
    armed_delete: bool,
    pub notice: String,
    pub loading: bool,
}

/// What the git forms hold: a branch (name, base) or a worktree (folder, branch).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitForm {
    worktree: bool,
    first: String,
    second: String,
}

/// What the topic forms hold: a new topic (name, partitions) or a message (text) for the open topic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopicForm {
    publish: bool,
    first: String,
    second: String,
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

impl ConfigPanel {
    pub fn load_skills(root: &Path) -> Task<Message> {
        let root = root.to_path_buf();
        Task::perform(async move { config_data::skills(&root) }, |r| {
            Message::Panel(crate::panel::PanelMsg::Cfg(CfgMsg::Skills(r)))
        })
    }

    pub fn load_git(root: &Path) -> Task<Message> {
        let root = root.to_path_buf();
        Task::perform(async move { config_data::git(&root) }, |r| {
            Message::Panel(crate::panel::PanelMsg::Cfg(CfgMsg::Git(r)))
        })
    }

    pub fn set_api(&mut self, api: &Path) {
        if self.api != api {
            self.api = api.to_path_buf();
        }
    }

    pub fn load_topics(api: &Path) -> Task<Message> {
        let api = api.to_path_buf();
        Task::perform(async move { config_data::topics(&api) }, |r| {
            Message::Panel(crate::panel::PanelMsg::Cfg(CfgMsg::Topics(r)))
        })
    }

    /// Skills, topics and models together, so the tab counters are known before those tabs are opened.
    pub fn load_counts(root: &Path, api: &Path) -> Task<Message> {
        Task::batch([
            Self::load_skills(root),
            Self::load_topics(api),
            Self::load_models(root),
        ])
    }

    /// The topics loaded so far, with their consumer groups.
    fn topic_partitions(&self, name: &str) -> u32 {
        self.topics
            .iter()
            .find(|t| t.name == name)
            .map_or(1, |t| t.partitions)
    }

    /// How many messages of the open topic to ask for.
    fn feed_limit(&self) -> usize {
        if self.feed_limit == 0 {
            50
        } else {
            self.feed_limit
        }
    }

    pub fn topic_rows(&self) -> &[TopicRow] {
        &self.topics
    }

    /// Worktrees of the repository, once the Git tab has loaded them.
    pub fn git_count(&self) -> Option<usize> {
        self.git.as_ref().map(|g| g.worktrees.len())
    }

    /// `(skills, topics, vendors)` loaded so far, for the counters on the tabs.
    pub fn counts(&self) -> (usize, usize, usize) {
        (
            self.skills.len(),
            self.topics.len(),
            self.models.vendors.len(),
        )
    }

    /// The global `config.yaml` (shown in the status bar), with the home folder as `~`.
    pub fn config_path(&self) -> Option<String> {
        let path = display(&self.models.target);
        if path.is_empty() {
            return None;
        }
        Some(match std::env::var("HOME") {
            Ok(home) if !home.is_empty() && path.starts_with(&home) => {
                format!("~{}", &path[home.len()..])
            }
            _ => path,
        })
    }

    /// Looks again at the topics and at the open topic's messages (called when the state files change).
    pub fn reload_events(&self) -> Task<Message> {
        let mut tasks = vec![Self::load_topics(&self.api)];
        if let Some(name) = self.topic.clone() {
            let api = self.api.clone();
            let limit = self.feed_limit();
            tasks.push(Task::perform(
                async move { config_data::topic_messages(&api, &name, limit) },
                |r| cfg(CfgMsg::Feed(r)),
            ));
        }
        Task::batch(tasks)
    }

    pub fn load_models(root: &Path) -> Task<Message> {
        let root = root.to_path_buf();
        Task::perform(async move { config_data::models(&root) }, |r| {
            Message::Panel(crate::panel::PanelMsg::Cfg(CfgMsg::Models(r)))
        })
    }

    /// The form's state as text, to tell whether anything changed since it was opened.
    fn snapshot(&self) -> String {
        format!(
            "{:?}{:?}{}{}{}{}",
            self.vfields,
            self.rfields,
            self.vendor.mcp.supported,
            self.vendor.disabled,
            self.role.worktree,
            self.role.can_spawn
        )
    }

    /// Are there changes that `Save to config.yaml` would write?
    fn dirty(&self) -> bool {
        match &self.editing {
            Some(Editing::Vendor(_) | Editing::Role(_)) => {
                self.is_new || self.snapshot() != self.baseline
            }
            Some(Editing::Roles) => !self.role_pending.is_empty() || !self.role_removed.is_empty(),
            _ => false,
        }
    }

    /// The role as the table shows it: with the change that is waiting to be saved, if any.
    fn role_now(&self, name: &str) -> Role {
        self.role_pending
            .get(name)
            .or_else(|| self.models.roles.get(name))
            .cloned()
            .unwrap_or_default()
    }

    /// The models a vendor lists (its own list and its default).
    fn vendor_models(&self, vendor: &str) -> Vec<String> {
        let found = self.models.vendors.get(vendor);
        let mut models: Vec<String> = found.map(|v| v.models.clone()).unwrap_or_default();
        if let Some(v) = found {
            if !v.default_model.is_empty() && !models.contains(&v.default_model) {
                models.insert(0, v.default_model.clone());
            }
        }
        models
    }

    fn open_vendor(&mut self, name: &str, new: bool) {
        self.editing = Some(Editing::Vendor(name.to_owned()));
        self.vendor = self.models.vendors.get(name).cloned().unwrap_or_default();
        self.is_new = new;
        self.armed_delete = false;
        let v = &self.vendor;
        self.vfields = vec![
            (VField::Name, name.to_owned()),
            (VField::Command, v.command.clone()),
            (VField::Args, v.args.join(" ")),
            (VField::Models, v.models.join(", ")),
            (VField::DefaultModel, v.default_model.clone()),
            (VField::ModelFlag, v.model_flag.clone()),
            (VField::ShepherdAgent, v.shepherd_agent.clone()),
        ];
        self.baseline = self.snapshot();
    }

    fn open_role(&mut self, name: &str, new: bool) {
        self.editing = Some(Editing::Role(name.to_owned()));
        self.role = self.models.roles.get(name).cloned().unwrap_or_default();
        self.is_new = new;
        self.armed_delete = false;
        let r = &self.role;
        self.rfields = vec![
            (RField::Name, name.to_owned()),
            (RField::Vendor, r.vendor.clone()),
            (RField::Model, r.model.clone()),
            (RField::Fallback, r.fallback.join(", ")),
            (RField::MustDiffer, r.must_differ_from.clone()),
            (RField::Skills, r.skills.join(", ")),
            (RField::Prompt, r.prompt.clone()),
        ];
        self.baseline = self.snapshot();
    }

    pub fn update(&mut self, msg: CfgMsg, root: &Path) -> Task<Message> {
        self.fail = match &msg {
            CfgMsg::SkillNewSubmit | CfgMsg::GitFormSubmit | CfgMsg::TopicFormSubmit => {
                "Not created."
            }
            CfgMsg::SkillCustomize(_) | CfgMsg::SkillGlobal(..) | CfgMsg::SkillEditOpen(_) => {
                "Not changed."
            }
            CfgMsg::SkillSave | CfgMsg::Save => "Not saved.",
            CfgMsg::SkillDelete(_) | CfgMsg::Delete => "Not deleted.",
            CfgMsg::WorktreeRemove(_) => "Not removed.",
            CfgMsg::ComposerSend => "Not sent.",
            _ => self.fail,
        };
        if !matches!(msg, CfgMsg::Delete) {
            self.armed_delete = false;
        }
        match msg {
            CfgMsg::Skills(Ok(skills)) => {
                if std::mem::take(&mut self.announce_skills) {
                    self.notice =
                        crate::ui::info(&format!("Reloaded {} skills from disk.", skills.len()));
                }
                self.skills = skills;
                self.loading = false;
                // picture aid: `SHEPHERD_NATIVE_SKILL=new` | `customize:<name>` | `edit:<name>`, applied once
                static PICTURE_AID_DONE: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if let Ok(spec) = std::env::var("SHEPHERD_NATIVE_SKILL") {
                    if !PICTURE_AID_DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let msg = match spec.split_once(':') {
                            Some(("customize", n)) => Some(CfgMsg::SkillCustomize(n.to_owned())),
                            Some(("edit", n)) => Some(CfgMsg::SkillEditOpen(n.to_owned())),
                            None if spec == "new" => Some(CfgMsg::SkillNewOpen),
                            _ => None,
                        };
                        if let Some(msg) = msg {
                            if let Some(name) = spec.split_once(':').map(|(_, n)| n.to_owned()) {
                                self.skill = Some(name);
                            }
                            return self.update(msg, root);
                        }
                    }
                }
            }
            CfgMsg::Git(Ok(git)) => {
                self.git = Some(git);
                self.loading = false;
            }
            CfgMsg::Topics(Ok(topics)) => {
                if std::mem::take(&mut self.announce_topics) {
                    self.notice =
                        crate::ui::info(&format!("Reread {} topics from the bus.", topics.len()));
                }
                self.topics = topics;
                self.loading = false;
                // like the other tabs, open the first topic instead of an empty detail
                if self.topic.is_none() {
                    if let Some(first) = self.topics.first().map(|t| t.name.clone()) {
                        return self.update(CfgMsg::SelectTopic(first), root);
                    }
                }
            }
            CfgMsg::Models(Ok(models)) => {
                self.models = models;
                self.loading = false;
                if self.editing.is_none() {
                    if let Some(name) = self.models.vendors.keys().next().cloned() {
                        self.open_vendor(&name, false);
                    }
                }
                // picture aid: `SHEPHERD_NATIVE_MODELS=installed` | `role:<name>` | `picker:<name>`, applied once
                static MODELS_AID_DONE: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if let Ok(spec) = std::env::var("SHEPHERD_NATIVE_MODELS") {
                    if !MODELS_AID_DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        if spec == "installed" {
                            self.editing = Some(Editing::Installed);
                        } else if spec == "roles" {
                            self.editing = Some(Editing::Roles);
                        } else if spec == "dirty" {
                            self.open_vendor("my-vendor", true);
                        } else if let Some(role) = spec.strip_prefix("role:") {
                            self.open_role(role, false);
                        } else if let Some(role) = spec.strip_prefix("picker:") {
                            self.open_role(role, false);
                            let _ = self.update(CfgMsg::SkillPickOpen, root);
                        }
                    }
                }
                // what is installed depends on the vendors, and asking the shell and the agents takes a moment
                let vendors = self.models.vendors.clone();
                return Task::perform(async move { installed::detect(&vendors) }, |rows| {
                    cfg(CfgMsg::Installed(rows))
                });
            }
            CfgMsg::Installed(rows) => {
                if self.looking_again {
                    self.looking_again = false;
                    self.notice = crate::ui::info(&format!(
                        "Looked on PATH again: {} agent commands found.",
                        rows.iter().filter(|r| r.installed()).count()
                    ));
                }
                // the slow part comes next: some agents take seconds to list their models
                let asked = rows.clone();
                self.installed = rows;
                return Task::perform(async move { installed::live_for(&asked) }, |live| {
                    cfg(CfgMsg::InstalledLive(live))
                });
            }
            CfgMsg::InstalledLive(live) => installed::merge_live(&mut self.installed, &live),
            CfgMsg::SelectInstalled => {
                self.editing = Some(Editing::Installed);
                self.armed_delete = false;
            }
            CfgMsg::AddVendorFrom(command) => {
                let Some(row) = self
                    .installed
                    .iter()
                    .find(|r| r.command == command)
                    .cloned()
                else {
                    return Task::none();
                };
                let mut name = command.clone();
                let mut n = 2;
                while self.models.vendors.contains_key(&name) {
                    name = format!("{command}-{n}");
                    n += 1;
                }
                self.open_vendor(&name, true);
                let known = installed::KNOWN_KINDS.contains(&command.as_str());
                self.vendor.command = command.clone();
                self.vendor.models = row.models.clone();
                self.vendor.default_model = row.default_model.clone().unwrap_or_default();
                if known {
                    self.vendor.shepherd_agent = command.clone();
                }
                for (field, value) in [
                    (VField::Command, command),
                    (VField::Models, row.models.join(", ")),
                    (VField::DefaultModel, self.vendor.default_model.clone()),
                    (VField::ShepherdAgent, self.vendor.shepherd_agent.clone()),
                ] {
                    if let Some(slot) = self.vfields.iter_mut().find(|(f, _)| *f == field) {
                        slot.1 = value;
                    }
                }
                self.notice = "Filled in from what is installed: pick a name and Save.".into();
            }
            CfgMsg::SkillPickOpen => {
                let selected = self
                    .rfields
                    .iter()
                    .find(|(f, _)| *f == RField::Skills)
                    .map(|(_, v)| words(v))
                    .unwrap_or_default();
                self.skill_picker = Some(SkillPicker {
                    selected,
                    filter: String::new(),
                });
            }
            CfgMsg::SkillPickToggle(name) => {
                if let Some(picker) = &mut self.skill_picker {
                    match picker.selected.iter().position(|s| *s == name) {
                        Some(at) => {
                            picker.selected.remove(at);
                        }
                        None => picker.selected.push(name),
                    }
                }
            }
            CfgMsg::SkillPickFilter(filter) => {
                if let Some(picker) = &mut self.skill_picker {
                    picker.filter = filter;
                }
            }
            CfgMsg::SkillPickCancel => self.skill_picker = None,
            CfgMsg::SkillPickDone => {
                let Some(picker) = self.skill_picker.take() else {
                    return Task::none();
                };
                return self.update(
                    CfgMsg::RoleField(RField::Skills, picker.selected.join(", ")),
                    root,
                );
            }
            CfgMsg::Skills(Err(e))
            | CfgMsg::Git(Err(e))
            | CfgMsg::Topics(Err(e))
            | CfgMsg::Models(Err(e))
            | CfgMsg::Feed(Err(e)) => {
                self.notice = e;
                self.loading = false;
            }
            CfgMsg::Feed(Ok(feed)) => {
                self.feed = feed;
                self.loading = false;
            }
            CfgMsg::SelectSkill(name) => {
                if self.skill_edit.as_ref().is_some_and(SkillEdit::dirty) {
                    self.notice = "Save or cancel the changes to the open skill first.".into();
                } else {
                    self.skill_edit = None;
                    self.skill_new = None;
                    self.armed_skill = None;
                    self.skill = Some(name);
                }
            }
            CfgMsg::SkillNewOpen => {
                self.skill_edit = None;
                self.skill_new = Some(SkillNew {
                    name: String::new(),
                    repo: false,
                    error: None,
                });
            }
            CfgMsg::SkillNewName(name) => {
                if let Some(form) = &mut self.skill_new {
                    form.name = name;
                    form.error = None;
                }
            }
            CfgMsg::SkillNewRepo(repo) => {
                if let Some(form) = &mut self.skill_new {
                    form.repo = repo;
                }
            }
            CfgMsg::SkillNewCancel => self.skill_new = None,
            CfgMsg::SkillNewSubmit => {
                let Some(form) = &mut self.skill_new else {
                    return Task::none();
                };
                let name = form.name.trim().to_owned();
                if name.is_empty() {
                    form.error = Some("A skill needs a name.".into());
                    return Task::none();
                }
                let repo = form.repo;
                match config_data::create_skill(root, &name, repo)
                    .and_then(|file| SkillEdit::open(&name, file))
                {
                    Ok(edit) => {
                        self.skill_new = None;
                        self.skill = Some(name.clone());
                        self.skill_edit = Some(edit);
                        self.notice = format!(
                            "Created skill {name} in {}.",
                            if repo { "this project" } else { "your skills" }
                        );
                        return Self::load_skills(root);
                    }
                    Err(why) => form.error = Some(why),
                }
            }
            CfgMsg::SkillCustomize(name) => {
                match config_data::customize_skill(root, &name)
                    .and_then(|file| SkillEdit::open(&name, file))
                {
                    Ok(edit) => {
                        self.skill_edit = Some(edit);
                        self.notice = format!("Copied {name} to your skills. Edit it here.");
                        return Self::load_skills(root);
                    }
                    Err(why) => self.notice = crate::panel::refusal(self.fail, &why),
                }
            }
            CfgMsg::SkillEditOpen(name) => {
                match config_data::editable_skill_file(root, &name)
                    .and_then(|file| SkillEdit::open(&name, file))
                {
                    Ok(edit) => self.skill_edit = Some(edit),
                    Err(why) => self.notice = crate::panel::refusal(self.fail, &why),
                }
            }
            CfgMsg::SkillEdit(action) => {
                if let Some(edit) = &mut self.skill_edit {
                    edit.content.perform(action);
                }
            }
            CfgMsg::SkillCancel => self.skill_edit = None,
            CfgMsg::SkillSave => {
                let Some(edit) = &mut self.skill_edit else {
                    return Task::none();
                };
                let text = edit.content.text();
                match config_data::save_skill(&edit.file, &text) {
                    Ok(problems) => {
                        edit.saved = text;
                        self.notice = if problems.is_empty() {
                            format!("Saved {}/SKILL.md.", edit.name)
                        } else {
                            format!(
                                "Saved {}/SKILL.md, with {} problem(s).",
                                edit.name,
                                problems.len()
                            )
                        };
                        edit.problems = problems;
                        return Self::load_skills(root);
                    }
                    Err(why) => self.notice = crate::panel::refusal(self.fail, &why),
                }
            }
            CfgMsg::SkillDelete(name) => {
                if self.armed_skill.as_deref() != Some(name.as_str()) {
                    self.armed_skill = Some(name);
                    return Task::none();
                }
                self.armed_skill = None;
                match config_data::delete_skill(root, &name) {
                    Ok(()) => {
                        self.skill_edit = None;
                        self.skill = None;
                        self.notice = format!("Deleted skill {name}.");
                        return Self::load_skills(root);
                    }
                    Err(why) => self.notice = crate::panel::refusal(self.fail, &why),
                }
            }
            CfgMsg::SkillGlobal(name, on) => match config_data::set_global_skill(root, &name, on) {
                Ok(()) => {
                    self.notice = if on {
                        format!("{name} is now global. Every agent gets it at its next start.")
                    } else {
                        format!("{name} is no longer global.")
                    };
                    return Self::load_skills(root);
                }
                Err(why) => self.notice = crate::panel::refusal(self.fail, &why),
            },
            CfgMsg::SelectWorktree(path) => {
                self.armed_worktree = None;
                self.worktree = Some(path);
            }
            CfgMsg::GitFormOpen(worktree) => {
                self.git_form = Some(GitForm {
                    worktree,
                    ..GitForm::default()
                });
            }
            CfgMsg::GitFormFirst(text) => {
                if let Some(form) = &mut self.git_form {
                    form.first = text;
                }
            }
            CfgMsg::GitFormSecond(text) => {
                if let Some(form) = &mut self.git_form {
                    form.second = text;
                }
            }
            CfgMsg::GitFormCancel => self.git_form = None,
            CfgMsg::GitFormSubmit => {
                let Some(form) = self.git_form.clone() else {
                    return Task::none();
                };
                if form.first.trim().is_empty() || (form.worktree && form.second.trim().is_empty())
                {
                    self.notice = crate::ui::invalid(if form.worktree {
                        "A worktree needs a folder and a branch."
                    } else {
                        "A branch needs a name."
                    });
                    return Task::none();
                }
                self.git_form = None;
                let (api, root) = (self.api.clone(), root.display().to_string());
                return Task::perform(
                    async move {
                        if form.worktree {
                            actions::git_worktree_add(&api, &form.first, &form.second, &root).map(
                                |dir| format!("Created worktree {dir} on {}.", form.second.trim()),
                            )
                        } else {
                            actions::git_branch(&api, &form.first, &form.second, &root).map(|_| {
                                let base = form.second.trim();
                                format!(
                                    "Created branch {} from {}.",
                                    form.first.trim(),
                                    if base.is_empty() {
                                        "the current commit"
                                    } else {
                                        base
                                    }
                                )
                            })
                        }
                    },
                    |r| cfg(CfgMsg::GitDone(r)),
                );
            }
            CfgMsg::WorktreeRemove(path) => {
                if self.armed_worktree.as_deref() != Some(path.as_str()) {
                    self.armed_worktree = Some(path);
                    return Task::none();
                }
                self.armed_worktree = None;
                let (api, root) = (self.api.clone(), root.display().to_string());
                return Task::perform(
                    async move {
                        actions::git_worktree_remove(&api, &path, &root)
                            .map(|_| format!("Removed worktree {path}. The branch is kept."))
                    },
                    |r| cfg(CfgMsg::GitDone(r)),
                );
            }
            CfgMsg::TopicFormOpen(publish) => {
                if publish && self.topic.is_none() {
                    self.notice = "Select a topic first".into();
                    return Task::none();
                }
                self.topic_form = Some(TopicForm {
                    publish,
                    ..TopicForm::default()
                });
            }
            CfgMsg::TopicFormFirst(text) => {
                if let Some(form) = &mut self.topic_form {
                    form.first = text;
                }
            }
            CfgMsg::TopicFormSecond(text) => {
                if let Some(form) = &mut self.topic_form {
                    form.second = text;
                }
            }
            CfgMsg::TopicFormCancel => self.topic_form = None,
            CfgMsg::TopicFormSubmit => {
                let Some(form) = self.topic_form.clone() else {
                    return Task::none();
                };
                let api = self.api.clone();
                if form.publish {
                    let (Some(topic), false) = (self.topic.clone(), form.first.trim().is_empty())
                    else {
                        self.notice = crate::ui::invalid("A message cannot be empty.");
                        return Task::none();
                    };
                    self.topic_form = None;
                    return Task::perform(
                        async move {
                            actions::topic_publish(&api, &topic, &form.first, None)
                                .map(|_| format!("Sent to #{topic}."))
                        },
                        |r| cfg(CfgMsg::TopicDone(r)),
                    );
                }
                let partitions = match form.second.trim() {
                    "" => Some(1),
                    n => n.parse::<u32>().ok().filter(|n| *n > 0),
                };
                let (name, Some(partitions)) = (form.first.trim().to_owned(), partitions) else {
                    self.notice = crate::ui::invalid("Not created: partitions must be 1 or more.");
                    return Task::none();
                };
                if name.is_empty() {
                    self.notice = crate::ui::invalid("A topic needs a name.");
                    return Task::none();
                }
                self.topic_form = None;
                return Task::perform(
                    async move {
                        actions::topic_create(&api, &name, partitions).map(|_| {
                            format!(
                                "Created topic #{name} with {partitions} partition{}.",
                                if partitions == 1 { "" } else { "s" }
                            )
                        })
                    },
                    |r| cfg(CfgMsg::TopicDone(r)),
                );
            }
            CfgMsg::TopicDone(result) => {
                self.notice = match result {
                    Ok(done) => done,
                    Err(why) => crate::panel::refusal(self.fail, &why),
                };
                return self.reload_events();
            }
            CfgMsg::GitDone(result) => {
                self.notice = match result {
                    Ok(done) => done,
                    Err(why) => crate::panel::refusal(self.fail, &why),
                };
                return Self::load_git(root);
            }
            CfgMsg::OpenAgent(id) => {
                return Task::batch([
                    Task::done(Message::Panel(PanelMsg::Tab(Tab::Agents))),
                    Task::done(Message::Panel(PanelMsg::SelectAgent(id))),
                ]);
            }
            CfgMsg::MsgPartition(p) => self.msg_partition = p,
            CfgMsg::ComposerText(t) => self.composer = t,
            CfgMsg::ComposerPartition(p) => self.composer_partition = p,
            CfgMsg::LoadOlder => {
                let Some(name) = self.topic.clone() else {
                    return Task::none();
                };
                self.feed_limit = (self.feed_limit() + 50).min(500);
                let (api, limit) = (self.api.clone(), self.feed_limit());
                return Task::perform(
                    async move { config_data::topic_messages(&api, &name, limit) },
                    |r| cfg(CfgMsg::Feed(r)),
                );
            }
            CfgMsg::ComposerSend => {
                let text = self.composer.trim().to_owned();
                let Some(topic) = self.topic.clone() else {
                    self.notice = crate::ui::invalid("Pick a topic to write to.");
                    return Task::none();
                };
                if text.is_empty() {
                    self.notice =
                        crate::ui::invalid("Write something first. Empty messages are not sent.");
                    return Task::none();
                }
                self.composer.clear();
                let api = self.api.clone();
                let partition = self.composer_partition;
                return Task::perform(
                    async move {
                        actions::topic_publish(&api, &topic, &text, partition).map(|_| {
                            match partition {
                                Some(p) => format!("Sent to #{topic} p{p}."),
                                None => format!("Sent to #{topic}."),
                            }
                        })
                    },
                    |r| cfg(CfgMsg::TopicDone(r)),
                );
            }
            CfgMsg::SelectTopic(name) => {
                self.topic = Some(name.clone());
                self.feed_limit = 0;
                self.composer_partition = None;
                self.loading = true;
                let api = self.api.clone();
                let limit = self.feed_limit();
                return Task::perform(
                    async move { config_data::topic_messages(&api, &name, limit) },
                    |r| cfg(CfgMsg::Feed(r)),
                );
            }
            CfgMsg::SelectVendor(name) => self.open_vendor(&name, false),
            CfgMsg::SelectRole(name) => self.open_role(&name, false),
            CfgMsg::SelectRoles => {
                self.editing = Some(Editing::Roles);
                self.armed_delete = false;
            }
            CfgMsg::RolesVendor(name, vendor) => {
                let mut role = self.role_now(&name);
                role.model = self
                    .vendor_models(&vendor)
                    .first()
                    .cloned()
                    .unwrap_or_default();
                role.vendor = vendor;
                self.role_pending.insert(name, role);
            }
            CfgMsg::RolesModel(name, model) => {
                let mut role = self.role_now(&name);
                role.model = model;
                self.role_pending.insert(name, role);
            }
            CfgMsg::RolesRemove(name) => {
                self.role_pending.remove(&name);
                self.role_removed.insert(name);
            }
            CfgMsg::Discard => {
                self.role_pending.clear();
                self.role_removed.clear();
                match self.editing.clone() {
                    Some(Editing::Vendor(name)) if !self.is_new => self.open_vendor(&name, false),
                    Some(Editing::Role(name)) if !self.is_new => self.open_role(&name, false),
                    Some(Editing::Vendor(_) | Editing::Role(_)) => {
                        self.editing = None;
                        if let Some(name) = self.models.vendors.keys().next().cloned() {
                            self.open_vendor(&name, false);
                        }
                    }
                    _ => {}
                }
                self.notice = crate::ui::info("Discarded your changes.");
            }
            CfgMsg::LookAgain => {
                self.looking_again = true;
                let vendors = self.models.vendors.clone();
                return Task::perform(async move { installed::detect(&vendors) }, |rows| {
                    cfg(CfgMsg::Installed(rows))
                });
            }
            CfgMsg::NewVendor => self.open_vendor("my-vendor", true),
            CfgMsg::NewRole => self.open_role("my-role", true),
            CfgMsg::VendorField(field, value) => {
                if let Some(slot) = self.vfields.iter_mut().find(|(f, _)| *f == field) {
                    slot.1 = value.clone();
                }
                let v = &mut self.vendor;
                match field {
                    VField::Name => {}
                    VField::Command => v.command = value,
                    VField::Args => v.args = words(&value),
                    VField::Models => v.models = words(&value),
                    VField::DefaultModel => v.default_model = value,
                    VField::ModelFlag => v.model_flag = value,
                    VField::ShepherdAgent => v.shepherd_agent = value,
                }
            }
            CfgMsg::VendorMcp(on) => self.vendor.mcp.supported = on,
            CfgMsg::VendorDisabled(on) => self.vendor.disabled = on,
            CfgMsg::RoleField(field, value) => {
                if let Some(slot) = self.rfields.iter_mut().find(|(f, _)| *f == field) {
                    slot.1 = value.clone();
                }
                let r = &mut self.role;
                match field {
                    RField::Name => {}
                    RField::Vendor => r.vendor = value,
                    RField::Model => r.model = value,
                    RField::Fallback => r.fallback = words(&value),
                    RField::MustDiffer => r.must_differ_from = value,
                    RField::Skills => r.skills = words(&value),
                    RField::Prompt => r.prompt = value,
                }
            }
            CfgMsg::RoleWorktree(on) => self.role.worktree = on,
            CfgMsg::RoleSpawn(on) => self.role.can_spawn = on,
            CfgMsg::Save => {
                let result = match &self.editing {
                    Some(Editing::Vendor(_)) => {
                        let name = self
                            .vfields
                            .iter()
                            .find(|(f, _)| *f == VField::Name)
                            .map(|(_, v)| v.trim().to_owned())
                            .unwrap_or_default();
                        config_data::save_vendor(&name, &self.vendor)
                            .map(|p| (p, Editing::Vendor(name)))
                    }
                    Some(Editing::Role(_)) => {
                        let name = self
                            .rfields
                            .iter()
                            .find(|(f, _)| *f == RField::Name)
                            .map(|(_, v)| v.trim().to_owned())
                            .unwrap_or_default();
                        config_data::save_role(&name, &self.role).map(|p| (p, Editing::Role(name)))
                    }
                    Some(Editing::Roles) => {
                        let mut done = Ok(PathBuf::new());
                        for (name, role) in &self.role_pending {
                            if self.role_removed.contains(name) {
                                continue;
                            }
                            if let Err(e) = config_data::save_role(name, role) {
                                done = Err(e);
                                break;
                            }
                        }
                        if done.is_ok() {
                            for name in &self.role_removed {
                                if let Err(e) = config_data::delete_entry("roles", name) {
                                    done = Err(e);
                                    break;
                                }
                            }
                        }
                        match done {
                            Ok(path) => {
                                self.role_pending.clear();
                                self.role_removed.clear();
                                self.notice = "Saved config.yaml.".into();
                                let _ = path;
                                return Self::load_models(root);
                            }
                            Err(e) => {
                                self.notice = crate::panel::refusal(self.fail, &e);
                                return Task::none();
                            }
                        }
                    }
                    Some(Editing::Installed) | None => return Task::none(),
                };
                match result {
                    Ok((_, editing)) => {
                        self.notice = "Saved config.yaml.".into();
                        self.editing = Some(editing);
                        self.is_new = false;
                        self.baseline = self.snapshot();
                        return Self::load_models(root);
                    }
                    Err(e) => self.notice = crate::panel::refusal(self.fail, &e),
                }
            }
            CfgMsg::Delete => {
                if !self.armed_delete {
                    self.armed_delete = true;
                    return Task::none();
                }
                self.armed_delete = false;
                if matches!(
                    self.editing,
                    Some(Editing::Installed | Editing::Roles) | None
                ) {
                    return Task::none();
                }
                let result = match self.editing.take() {
                    Some(Editing::Vendor(name)) => config_data::delete_entry("vendors", &name),
                    Some(Editing::Role(name)) => config_data::delete_entry("roles", &name),
                    Some(Editing::Installed | Editing::Roles) | None => return Task::none(),
                };
                match result {
                    Ok(path) => {
                        self.notice = format!("Deleted from {}", display(&path));
                        return Self::load_models(root);
                    }
                    Err(e) => self.notice = crate::panel::refusal(self.fail, &e),
                }
            }
            CfgMsg::RefreshEvents => {
                self.announce_topics = true;
                return self.update(CfgMsg::Refresh, root);
            }
            CfgMsg::RefreshSkills => {
                self.announce_skills = true;
                return self.update(CfgMsg::Refresh, root);
            }
            CfgMsg::Refresh => {
                self.loading = true;
                return Task::batch([
                    Self::load_git(root),
                    Self::load_skills(root),
                    Self::load_topics(&self.api),
                    Self::load_models(root),
                ]);
            }
        }
        Task::none()
    }

    // ---- views -------------------------------------------------------------------------------------------------

    pub fn skills_view<'a>(
        &'a self,
        filter: &str,
        chip: usize,
        agents: &'a [shepherd_app_core::shepherd_core::Agent],
    ) -> Element<'a, Message> {
        if self.loading && self.skills.is_empty() {
            return loading_state("Loading skills…");
        }
        let builtin = |s: &SkillRow| s.scope == "Default";
        let used_by = |name: &str| -> Vec<&str> {
            agents
                .iter()
                .filter(|a| a.active_skills.iter().any(|x| x == name))
                .map(|a| a.id.as_str())
                .collect()
        };
        let counts = [
            self.skills.len(),
            self.skills.iter().filter(|s| builtin(s)).count(),
            self.skills.iter().filter(|s| !builtin(s)).count(),
            self.skills.iter().filter(|s| s.global).count(),
        ];
        let chips = [
            ("All", counts[0]),
            ("Built in", counts[1]),
            ("Yours", counts[2]),
            ("Global", counts[3]),
        ];
        let pass = |s: &&SkillRow| {
            let f = filter.to_lowercase();
            let hay = format!("{} {}", s.name, s.description).to_lowercase();
            let chip_ok = match chip {
                1 => builtin(s),
                2 => !builtin(s),
                3 => s.global,
                _ => true,
            };
            chip_ok && f.split_whitespace().all(|w| hay.contains(w))
        };
        let entry = |s: &'a SkillRow| -> Element<'a, Message> {
            let users = used_by(&s.name).len();
            let body = column![
                row![
                    text(format!("{}{}", if s.enabled { "" } else { "✖ " }, s.name))
                        .font(crate::theme::MONO_BOLD)
                        .size(13)
                        .color(crate::theme::HI)
                        .width(Length::Fill),
                    text(if users > 0 {
                        format!("{users} agent{}", if users == 1 { "" } else { "s" })
                    } else {
                        String::new()
                    })
                    .size(11)
                    .color(DIM),
                ],
                text(s.description.chars().take(46).collect::<String>())
                    .size(11)
                    .color(DIM),
            ]
            .spacing(3);
            button(body)
                .width(Length::Fill)
                .padding([8, 10])
                .style(item_style(self.skill.as_deref() == Some(s.name.as_str())))
                .on_press(crate::ui::nav(
                    self.skill.as_deref() == Some(s.name.as_str()),
                    cfg(CfgMsg::SelectSkill(s.name.clone())),
                ))
                .into()
        };
        let heading = |s: String| {
            container(text(s).size(11.5).color(DIM)).padding(crate::theme::pad(10.0, 4.0, 4.0, 4.0))
        };
        let (own, shipped): (Vec<&SkillRow>, Vec<&SkillRow>) =
            self.skills.iter().filter(pass).partition(|s| !builtin(s));
        let mut items: Vec<Element<'a, Message>> = vec![];
        if !own.is_empty() {
            items.push(heading(format!("Yours ({})", own.len())).into());
            items.extend(own.into_iter().map(entry));
        }
        if !shipped.is_empty() {
            items.push(heading(format!("Built in ({})", shipped.len())).into());
            items.extend(shipped.into_iter().map(entry));
        }
        let detail: Element<'a, Message> = match self
            .skill
            .as_deref()
            .and_then(|n| self.skills.iter().find(|s| s.name == n))
            .or_else(|| self.skills.first())
        {
            None => empty_state(
                "No skills found",
                "Skills will appear here when Shepherd discovers them in this project.",
            ),
            Some(s) => {
                let mut col = column![
                    row![
                        text(s.name.clone())
                            .font(crate::theme::MONO_BOLD)
                            .size(20)
                            .color(crate::theme::HI),
                        crate::panel::badge(
                            if !s.enabled {
                                "disabled".to_owned()
                            } else if s.scope == "Default" {
                                "built in, read-only".to_owned()
                            } else if s.global {
                                "yours, global".to_owned()
                            } else {
                                "yours".to_owned()
                            },
                            if s.enabled {
                                crate::theme::IDLE
                            } else {
                                crate::theme::BAD
                            }
                        ),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                    text(s.description.clone()).size(13).color(DIM)
                ]
                .spacing(10);
                for problem in &s.problems {
                    col = col.push(crate::panel::alert(
                        text(format!("✕ {problem}"))
                            .size(13)
                            .color(crate::theme::BAD)
                            .into(),
                        crate::theme::BAD,
                    ));
                }
                // what can be done with it: the ones that ship with Shepherd are customized (copied), not edited
                let editable = s.scope != "Default";
                let armed = self.armed_skill.as_deref() == Some(s.name.as_str());
                let mut actions = row![].spacing(10);
                if editable {
                    actions = actions
                        .push(cfg_btn("Edit", CfgMsg::SkillEditOpen(s.name.clone()), true))
                        .push(if armed {
                            button(
                                container(text("Confirm: delete this skill").size(13))
                                    .center_y(Length::Fill),
                            )
                            .padding([0, 12])
                            .height(32)
                            .style(crate::theme::danger_button_style())
                            .on_press(cfg(CfgMsg::SkillDelete(s.name.clone())))
                        } else {
                            button(container(text("Delete").size(13)).center_y(Length::Fill))
                                .padding([0, 12])
                                .height(32)
                                .style(crate::theme::outline_button_style(false))
                                .on_press(cfg(CfgMsg::SkillDelete(s.name.clone())))
                        });
                } else {
                    actions = actions
                        .push(cfg_btn(
                            "Customize",
                            CfgMsg::SkillCustomize(s.name.clone()),
                            true,
                        ))
                        .push(
                            text("ships with Shepherd: customizing copies it to your skills")
                                .size(11)
                                .color(DIM),
                        );
                }
                actions = actions.push(cfg_btn(
                    if s.global {
                        "Remove from global"
                    } else {
                        "Make global"
                    },
                    CfgMsg::SkillGlobal(s.name.clone(), !s.global),
                    false,
                ));
                let users = used_by(&s.name);
                let mut used = row![text("Used by").size(12).color(DIM)]
                    .spacing(10)
                    .align_y(iced::Alignment::Center);
                if users.is_empty() {
                    used = used.push(text("no agent yet").size(12).color(DIM));
                }
                for u in users {
                    used = used.push(
                        container(text(u.to_owned()).size(11.5))
                            .padding([2, 10])
                            .style(|_| container::Style {
                                border: iced::Border {
                                    color: crate::theme::LINE2,
                                    width: 1.0,
                                    radius: 12.0.into(),
                                },
                                ..container::Style::default()
                            }),
                    );
                }
                let lines = s.body.lines().count();
                // a 48 px gutter of line numbers (#4A4A5C); the front matter lines `name:` and `description:` are
                // violet, headings bright
                let mut source = column![].spacing(2);
                for (i, l) in s.body.lines().enumerate() {
                    let color = if l.starts_with("name:") || l.starts_with("description:") {
                        crate::theme::VIOLET
                    } else if l.starts_with('#') {
                        crate::theme::HI
                    } else {
                        crate::theme::FG
                    };
                    source = source.push(
                        row![
                            text((i + 1).to_string())
                                .size(12)
                                .color(crate::theme::LINE3)
                                .width(48)
                                .align_x(iced::alignment::Horizontal::Right),
                            text(l.to_owned()).size(12.5).color(color),
                        ]
                        .spacing(14),
                    );
                }
                let note = if s.scope == "Default" {
                    "Customize makes your own copy, which you can edit. The built-in one stays as it is."
                } else if s.global {
                    "Global: every agent gets it, whatever its role."
                } else {
                    "Agents get it when their role lists it or its description fits."
                };
                col.push(actions)
                    .push(text(note).size(11.5).color(DIM))
                    .push(used.wrap())
                    .push(crate::panel::card(
                        "SKILL.md",
                        Some(
                            text(format!(
                                "{}  {lines} lines",
                                if s.scope == "Default" {
                                    "ships with Shepherd".to_owned()
                                } else {
                                    s.dir.clone()
                                }
                            ))
                            .size(11)
                            .color(DIM)
                            .into(),
                        ),
                        source.into(),
                    ))
                    .into()
            }
        };
        let top = crate::ui::filter_bar("Filter by name or what it does", filter, &chips, chip);
        let detail_pane: Element<'a, Message> =
            if let Some(form) = &self.skill_new {
                detail_box(self.skill_new_view(form))
            } else if let Some(edit) = self.skill_edit.as_ref().filter(|e| {
                self.skill.as_deref().or(Some(e.name.as_str())) == Some(e.name.as_str())
            }) {
                // the editor scrolls by itself (both ways), so it does not go inside the detail's scrollable
                container(self.skill_editor_view(edit))
                    .padding([20, 24])
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            } else {
                detail_box(detail)
            };
        row![
            crate::panel::list_box_with(Some(top), items, vec![]),
            detail_pane
        ]
        .height(Length::Fill)
        .into()
    }

    /// The form that creates a skill: a name and where it goes.
    fn skill_new_view<'a>(&'a self, form: &'a SkillNew) -> Element<'a, Message> {
        let mut col = column![
            text("New skill").size(18).color(crate::theme::HI),
            text("It starts from Shepherd's template; you edit it next.")
                .size(12)
                .color(DIM),
            column![
                crate::theme::label("NAME"),
                text_input("lowercase-with-dashes", &form.name)
                    .on_input(|n| cfg(CfgMsg::SkillNewName(n)))
                    .on_submit(cfg(CfgMsg::SkillNewSubmit))
                    .style(crate::theme::input_style)
                    .padding([8, 10])
                    .size(13),
            ]
            .spacing(6),
            column![
                crate::theme::label("WHERE"),
                row![
                    checkbox(!form.repo)
                        .label("for me (all projects)")
                        .on_toggle(|on| cfg(CfgMsg::SkillNewRepo(!on)))
                        .size(14)
                        .text_size(13),
                    checkbox(form.repo)
                        .label("in this project")
                        .on_toggle(|on| cfg(CfgMsg::SkillNewRepo(on)))
                        .size(14)
                        .text_size(13),
                ]
                .spacing(24),
            ]
            .spacing(8),
        ]
        .spacing(14);
        if let Some(error) = &form.error {
            col = col.push(crate::panel::alert(
                text(format!("✕ {error}"))
                    .size(13)
                    .color(crate::theme::BAD)
                    .into(),
                crate::theme::BAD,
            ));
        }
        col.push(
            row![
                cfg_btn("Create", CfgMsg::SkillNewSubmit, true),
                cfg_btn("Cancel", CfgMsg::SkillNewCancel, false),
            ]
            .spacing(10),
        )
        .max_width(640)
        .into()
    }

    /// The editor of a skill: title, what the validation found, Save and Cancel, then the text with line numbers.
    fn skill_editor_view<'a>(&'a self, edit: &'a SkillEdit) -> Element<'a, Message> {
        let dirty = edit.dirty();
        let mut head = row![
            text(format!("EDIT {}", edit.name))
                .size(18)
                .color(crate::theme::HI),
            text("SKILL.md").size(11).color(DIM),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);
        if dirty {
            head = head.push(text("● unsaved changes").size(12).color(WARN));
        }
        head = head
            .push(container(text("")).width(Length::Fill))
            .push(cfg_btn("Save", CfgMsg::SkillSave, true))
            .push(cfg_btn(
                if dirty { "Discard" } else { "Close" },
                CfgMsg::SkillCancel,
                false,
            ));
        let mut col = column![head.spacing(10)].spacing(12);
        for problem in &edit.problems {
            col = col.push(crate::panel::alert(
                text(format!("✕ {problem}"))
                    .size(13)
                    .color(crate::theme::BAD)
                    .into(),
                crate::theme::BAD,
            ));
        }
        col.push(crate::editor::view(&edit.content, |a| {
            cfg(CfgMsg::SkillEdit(a))
        }))
        .height(Length::Fill)
        .into()
    }

    /// Git: one flexible region whose table keeps its fixed columns (1120 px), scrolling when narrower.
    pub fn git_view<'a>(&'a self) -> Element<'a, Message> {
        crate::ui::flex(1120.0, [20.0, 28.0], move || self.git_content())
    }

    fn git_content<'a>(&'a self) -> Element<'a, Message> {
        let Some(git) = &self.git else {
            if self.loading {
                return loading_state("Loading Git worktrees…");
            }
            return row![
                list_box(vec![], vec![refresh_button()]),
                detail_box(text("Loading…").color(DIM).into())
            ]
            .into();
        };
        let mut note = column![].spacing(4);
        if let Some(error) = &git.error {
            note = note.push(crate::panel::alert(
                text(format!("✕ {error}"))
                    .size(12)
                    .color(crate::theme::BAD)
                    .into(),
                crate::theme::BAD,
            ));
        }
        if let Some(n) = git
            .gh_note
            .as_deref()
            .filter(|n| !n.is_empty())
            .or(if git.gh_available {
                None
            } else {
                Some("`gh` is not available: pull requests are hidden.")
            })
        {
            note = note.push(text(n.to_owned()).size(12).color(DIM));
        }
        if git.worktrees.is_empty() {
            return container(
                column![
                    note,
                    empty_state(
                        "No worktrees found",
                        "This project does not currently expose any Git worktrees.",
                    )
                ]
                .spacing(8),
            )
            .padding([16, 28])
            .into();
        }
        let selected = self
            .worktree
            .as_deref()
            .and_then(|p| git.worktrees.iter().find(|w| w.path == p))
            .unwrap_or(&git.worktrees[0]);
        // fixed columns: Branch 260, Worktree 260, Changes 140, Ahead / behind 120, Agents 160, Pull request 180
        let cell = |w: f32, e: Element<'a, Message>| container(e).width(w).clip(true);
        let head = |s: &'static str, w: f32| cell(w, text(s).size(11.5).color(DIM).into());
        let mut table = column![container(
            row![
                head("Branch", 260.0),
                head("Worktree", 260.0),
                head("Changes", 140.0),
                head("Ahead / behind", 120.0),
                head("Agents", 160.0),
                head("Pull request", 180.0)
            ]
            .spacing(10)
        )
        .padding([12, 16])]
        .spacing(0);
        for w in &git.worktrees {
            let sel = w.path == selected.path;
            let branch: Element<'a, Message> = row![
                text("⎇").size(12).color(DIM),
                text(if w.branch.is_empty() {
                    "(no branch)".to_owned()
                } else {
                    w.branch.clone()
                })
                .font(crate::theme::MONO_BOLD)
                .size(12.5)
                .color(crate::theme::HI),
                if w.is_main {
                    container(text("main checkout").size(10).color(ACCENT))
                        .padding([0, 6])
                        .style(|_| container::Style {
                            border: iced::Border {
                                color: ACCENT,
                                width: 1.0,
                                radius: 3.0.into(),
                            },
                            ..container::Style::default()
                        })
                } else {
                    container(text(""))
                },
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into();
            let changes: Element<'a, Message> = if w.is_clean {
                row![text("■").size(9).color(OK), text("clean").size(12)]
                    .spacing(6)
                    .into()
            } else {
                row![
                    text("■").size(9).color(WARN),
                    text(format!("{} uncommitted", w.dirty_count)).size(12)
                ]
                .spacing(6)
                .into()
            };
            let pr: Element<'a, Message> = match &w.pr {
                Some(p) => text(format!("#{} {}", p.number, p.state.to_lowercase()))
                    .size(12)
                    .color(if matches!(p.state.as_str(), "OPEN" | "MERGED") {
                        OK
                    } else {
                        DIM
                    })
                    .into(),
                None => text("none").size(12).color(DIM).into(),
            };
            let line = row![
                cell(260.0, branch),
                cell(
                    260.0,
                    text(display(Path::new(&w.path))).size(12).color(DIM).into()
                ),
                cell(140.0, changes),
                cell(
                    120.0,
                    text(format!("+{} / {}", w.ahead, w.behind)).size(12).into()
                ),
                cell(
                    160.0,
                    text(w.agent_id.clone().unwrap_or_else(|| "none".into()))
                        .size(12)
                        .into()
                ),
                cell(180.0, pr),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center);
            table = table.push(
                button(line)
                    .width(Length::Fill)
                    .height(44)
                    .padding([0, 16])
                    .style(move |_, status| {
                        let hot =
                            matches!(status, button::Status::Hovered | button::Status::Pressed);
                        button::Style {
                            background: (sel || hot)
                                .then_some(Background::Color(crate::theme::SELECTED)),
                            text_color: crate::theme::FG,
                            border: iced::Border {
                                color: if sel { ACCENT } else { Color::TRANSPARENT },
                                width: 0.0,
                                radius: 0.0.into(),
                            },
                            ..button::Style::default()
                        }
                    })
                    .on_press(cfg(CfgMsg::SelectWorktree(w.path.clone()))),
            );
        }
        let table = container(table).width(Length::Fill).style(card_style);
        // what changed in the picked worktree
        let w = selected;
        let mut files = column![].spacing(0);
        if w.changes.is_empty() {
            files = files.push(
                text(if w.is_clean {
                    "Nothing to commit."
                } else {
                    "The files could not be listed."
                })
                .size(12)
                .color(DIM),
            );
        }
        for ch in &w.changes {
            let color = match ch.status.as_str() {
                "added" | "untracked" => OK,
                "deleted" => crate::theme::BAD,
                _ => WARN,
            };
            files = files.push(
                container(
                    row![
                        text(ch.status.clone()).size(12).color(color).width(90),
                        text(ch.path.clone()).size(12).width(Length::Fill),
                        text(format!("+{}", ch.added)).size(11.5).color(OK),
                        text(format!("-{}", ch.removed))
                            .size(11.5)
                            .color(crate::theme::BAD),
                    ]
                    .spacing(8),
                )
                .padding([8, 0]),
            );
        }
        let changes_card = crate::panel::card(
            "CHANGES",
            Some(
                text(format!(
                    "{}  {}",
                    if w.branch.is_empty() {
                        "(detached)"
                    } else {
                        &w.branch
                    },
                    if w.is_clean {
                        "clean".to_owned()
                    } else {
                        format!("{} uncommitted", w.dirty_count)
                    }
                ))
                .size(11.5)
                .color(if w.is_clean { OK } else { WARN })
                .into(),
            ),
            files.into(),
        );
        let kv = |k: &'static str, v: String, color: Color| {
            row![
                text(k).size(12).color(DIM).width(90),
                text(v).size(12).color(color)
            ]
            .spacing(8)
        };
        let mut info = column![
            kv("folder", display(Path::new(&w.path)), crate::theme::FG),
            kv(
                "upstream",
                w.upstream
                    .clone()
                    .unwrap_or_else(|| "not pushed yet".into()),
                crate::theme::FG
            ),
            kv(
                "agents",
                w.agent_id.clone().unwrap_or_else(|| "none".into()),
                crate::theme::FG
            ),
            kv(
                "pull request",
                match &w.pr {
                    Some(p) => format!("#{} {} ({})", p.number, p.title, p.state.to_lowercase()),
                    None => "none".into(),
                },
                crate::theme::FG
            ),
        ]
        .spacing(6);
        if let Some(agent) = &w.agent_id {
            info = info.push(crate::panel::btn_link(
                "Open agent workspace",
                crate::panel::PanelMsg::Cfg(CfgMsg::OpenAgent(agent.clone())),
            ));
        }
        if !w.is_main {
            let armed = self.armed_worktree.as_deref() == Some(w.path.as_str());
            info = info.push(cfg_btn(
                if armed {
                    "Confirm: remove this worktree (the branch stays)"
                } else {
                    "Remove worktree"
                },
                CfgMsg::WorktreeRemove(w.path.clone()),
                armed,
            ));
            let mut why = vec![];
            if !w.is_clean {
                why.push(format!("it has {} uncommitted changes", w.dirty_count));
            }
            if let Some(a) = &w.agent_id {
                why.push(format!("agent {a} works in it"));
            }
            if !why.is_empty() {
                info = info.push(
                    text(format!(
                        "The server will refuse this: {}.",
                        why.join(" and ")
                    ))
                    .size(11.5)
                    .color(WARN),
                );
            }
        }
        let worktree_card = crate::panel::card("WORKTREE", None, info.into());
        let lower: Element<'a, Message> = match &self.git_form {
            Some(form) => self.git_form_view(form),
            None => row![
                container(changes_card).width(Length::FillPortion(3)),
                container(worktree_card).width(Length::FillPortion(2)),
            ]
            .spacing(16)
            .into(),
        };
        column![note, table, lower].spacing(16).into()
    }

    /// The one form for a new branch (name, base) or a new worktree (folder, branch).
    fn git_form_view<'a>(&'a self, form: &'a GitForm) -> Element<'a, Message> {
        let (title, first, second, hint) = if form.worktree {
            (
                "New worktree",
                ("FOLDER", "../my-feature"),
                ("BRANCH", "feat/my-feature"),
                "The branch is created from the current commit when it does not exist. A relative folder is taken from the project.",
            )
        } else {
            (
                "New branch",
                ("NAME", "feat/my-feature"),
                ("BASE · EMPTY FOR THE CURRENT COMMIT", "main"),
                "Creates the branch without switching to it.",
            )
        };
        let field =
            |label: &'static str, hint: &'static str, value: &'a str, on: fn(String) -> CfgMsg| {
                column![
                    crate::theme::label(label),
                    text_input(hint, value)
                        .on_input(move |t| cfg(on(t)))
                        .on_submit(cfg(CfgMsg::GitFormSubmit))
                        .style(crate::theme::input_style_marked(
                            crate::ui::is_invalid(&self.notice)
                                && value.trim().is_empty()
                                && !label.contains("EMPTY"),
                        ))
                        .padding([8, 10])
                        .size(13),
                ]
                .spacing(6)
            };
        container(
            column![
                text(title).size(18).color(crate::theme::HI),
                field(first.0, first.1, &form.first, CfgMsg::GitFormFirst),
                field(second.0, second.1, &form.second, CfgMsg::GitFormSecond),
                text(hint).size(11).color(DIM),
                row![
                    cfg_btn("Create", CfgMsg::GitFormSubmit, true),
                    cfg_btn("Cancel", CfgMsg::GitFormCancel, false),
                ]
                .spacing(10),
            ]
            .spacing(12),
        )
        .max_width(600)
        .into()
    }

    /// The detail of the open topic (or the open form): the content of Events' flexible region.
    fn events_detail<'a>(&'a self, frame: u32) -> Element<'a, Message> {
        let detail: Element<'a, Message> = match self
            .topic
            .as_deref()
            .and_then(|n| self.topics.iter().find(|t| t.name == n))
        {
            None => empty_state(
                "No topic selected",
                "Select an event topic to inspect consumers, lag, and recent messages.",
            ),
            Some(t) => {
                let consumers: Vec<String> = t
                    .groups
                    .iter()
                    .flat_map(|g| g.members.iter().cloned())
                    .collect();
                let _ = &consumers;
                let mut col = column![row![
                    text(format!("#{}", t.name))
                        .size(18)
                        .color(crate::theme::VIOLET),
                    text(format!(
                        "{} messages, {} partitions, {} consumer groups",
                        t.messages_count,
                        t.partitions,
                        t.groups.len()
                    ))
                    .size(12)
                    .color(DIM),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center)]
                .spacing(16);
                // Partitions: messages (offset consumed + still unread, best known over the groups) and lag.
                let lag_of = |p: u32| -> u64 {
                    t.groups
                        .iter()
                        .map(|g| g.lag_by_partition.get(&p).copied().unwrap_or(0))
                        .sum()
                };
                let msgs_of = |p: u32| -> u64 {
                    let groups = t
                        .groups
                        .iter()
                        .map(|g| {
                            g.committed_offsets.get(&p).copied().unwrap_or(0)
                                + g.lag_by_partition.get(&p).copied().unwrap_or(0)
                        })
                        .max()
                        .unwrap_or(0);
                    // without consumers, the newest offset seen in the recent messages
                    let seen = self
                        .feed
                        .iter()
                        .filter(|m| m.partition == p)
                        .map(|m| m.offset + 1)
                        .max()
                        .unwrap_or(0);
                    groups.max(seen)
                };
                let max_msgs = (0..t.partitions).map(msgs_of).max().unwrap_or(0).max(1);
                let mut parts = column![].spacing(8).width(Length::Fill);
                for p in 0..t.partitions {
                    let (lag, msgs) = (lag_of(p), msgs_of(p));
                    // `lag` canvas: the stream is as long as the partition is full
                    let reach = 0.15 + 0.8 * msgs as f64 / max_msgs as f64;
                    parts = parts.push(
                        row![
                            text(format!("p{p}"))
                                .size(12)
                                .color(crate::theme::HI)
                                .width(26),
                            iced::widget::canvas::Canvas::new(crate::prism::Prism {
                                preset: crate::prism::Preset::lag(reach, f64::from(p) * 7.0),
                                frame: frame.wrapping_add(p * 3),
                            })
                            .width(Length::Fill)
                            .height(14),
                            row![
                                text(format!("{msgs} msgs · lag ")).size(12).color(DIM),
                                text(lag.to_string()).size(12).color(if lag > 1 {
                                    WARN
                                } else {
                                    OK
                                }),
                            ]
                            .width(116),
                        ]
                        .spacing(8)
                        .width(Length::Fill)
                        .align_y(iced::Alignment::Center),
                    );
                }
                // Flow: who writes (left), the partitions with their lag (middle), who reads (right)
                let mut senders: Vec<&str> = vec![];
                for m in &self.feed {
                    if !senders.contains(&m.from.as_str()) {
                        senders.push(&m.from);
                    }
                }
                let mut publishers = column![crate::theme::label("PUBLISHERS")].spacing(8);
                for s in &senders {
                    let color = crate::theme::emitter_color(s);
                    publishers = publishers.push(
                        container(
                            row![
                                text("■").size(8).color(color),
                                text(if *s == "user" { "you" } else { s }.to_owned()).size(12),
                            ]
                            .spacing(8)
                            .align_y(iced::Alignment::Center),
                        )
                        .padding([0, 10])
                        .height(32)
                        .align_y(iced::alignment::Vertical::Center)
                        .style(crate::theme::chip_box_style),
                    );
                }
                if senders.is_empty() {
                    publishers = publishers.push(text("nobody yet").size(12).color(DIM));
                }
                let mut readers = column![crate::theme::label("CONSUMER GROUPS")].spacing(8);
                for g in &t.groups {
                    let lag: u64 = g.lag_by_partition.values().sum();
                    readers =
                        readers.push(
                            container(
                                column![
                                    row![
                                        text(g.group.clone()).size(12.5).color(crate::theme::HI),
                                        container(text("")).width(Length::Fill),
                                        text(if lag == 0 {
                                            "read".to_owned()
                                        } else {
                                            format!("lag {lag}")
                                        })
                                        .size(11)
                                        .color(if lag > 1 { WARN } else { OK }),
                                    ],
                                    text(if g.members.is_empty() {
                                        "no members".to_owned()
                                    } else {
                                        g.members.join(", ")
                                    })
                                    .size(11)
                                    .color(DIM),
                                ]
                                .spacing(4),
                            )
                            .padding([8, 10])
                            .width(Length::Fill)
                            .style(crate::theme::chip_box_style),
                        );
                }
                if t.groups.is_empty() {
                    readers = readers.push(text("nobody reads yet").size(12).color(DIM));
                }
                col = col.push(crate::panel::card(
                    "FLOW",
                    Some(
                        text("who writes, how full, who reads")
                            .size(11)
                            .color(DIM)
                            .into(),
                    ),
                    row![
                        container(publishers).width(170),
                        column![crate::theme::label("PARTITIONS"), parts]
                            .spacing(8)
                            .width(Length::Fill),
                        container(readers).width(250),
                    ]
                    .spacing(20)
                    .into(),
                ));
                // Recent messages, newest first: time, who emitted it, and colours.
                let mut feed = column![].spacing(0);
                if self.feed.is_empty() {
                    feed = feed.push(text("No messages yet.").size(12).color(DIM));
                }
                for m in self
                    .feed
                    .iter()
                    .rev()
                    .filter(|m| self.msg_partition.is_none_or(|p| p == m.partition))
                {
                    feed = feed.push(crate::panel::message_row(m, true));
                }
                let mut chips = row![chip_btn(
                    "All".into(),
                    self.msg_partition.is_none(),
                    CfgMsg::MsgPartition(None)
                )]
                .spacing(6);
                for p in 0..t.partitions {
                    chips = chips.push(chip_btn(
                        format!("p{p}"),
                        self.msg_partition == Some(p),
                        CfgMsg::MsgPartition(Some(p)),
                    ));
                }
                if self.feed.is_empty() {
                    feed = column![text(format!(
                        "No messages here yet. Send the first one below; agents reading #{} get it right away.",
                        t.name
                    ))
                    .size(12)
                    .color(DIM)];
                } else if self.feed.len() >= self.feed_limit() {
                    feed = feed.push(
                        container(cfg_btn("Load older", CfgMsg::LoadOlder, false)).padding([10, 0]),
                    );
                }
                col = col.push(crate::panel::card(
                    "MESSAGES · NEWEST FIRST",
                    Some(chips.into()),
                    feed.into(),
                ));
                col.max_width(1000).into()
            }
        };
        let detail = match &self.topic_form {
            Some(form) => self.topic_form_view(form),
            None => detail,
        };
        detail
    }

    pub fn events_view<'a>(&'a self, frame: u32, filter: &str) -> Element<'a, Message> {
        if self.loading && self.topics.is_empty() {
            return loading_state("Loading event topics…");
        }
        let items = self
            .topics
            .iter()
            .filter(|t| {
                let f = filter.to_lowercase();
                f.split_whitespace()
                    .all(|w| t.name.to_lowercase().contains(w))
            })
            .map(|t| {
                let unread: u64 = t
                    .groups
                    .iter()
                    .flat_map(|g| g.lag_by_partition.values())
                    .sum();
                let mut title = row![text(format!("#{}", t.name))
                    .size(13)
                    .color(crate::theme::HI)]
                .spacing(8)
                .align_y(iced::Alignment::Center);
                if t.paused {
                    title = title.push(text("paused").size(11).color(WARN));
                }
                if unread > 0 {
                    title = title.push(text(format!("{unread} unread")).size(11).color(WARN));
                }
                let body = column![
                    title,
                    text(format!(
                        "{} messages · {} partition(s)",
                        t.messages_count, t.partitions
                    ))
                    .size(11)
                    .color(DIM)
                ]
                .spacing(1);
                button(body)
                    .width(Length::Fill)
                    .padding([4, 8])
                    .style(item_style(self.topic.as_deref() == Some(t.name.as_str())))
                    .on_press(crate::ui::nav(
                        self.topic.as_deref() == Some(t.name.as_str()),
                        cfg(CfgMsg::SelectTopic(t.name.clone())),
                    ))
                    .into()
            })
            .collect();
        let detail = crate::ui::flex(760.0, [20.0, 28.0], move || self.events_detail(frame));
        let footer = vec![
            cfg_btn("+ Topic", CfgMsg::TopicFormOpen(false), false),
            cfg_btn("Send message", CfgMsg::TopicFormOpen(true), false),
            refresh_button(),
        ];
        let _ = footer;
        let top = crate::ui::filter_bar("Filter topics", filter, &[], 0);
        // the box to write to the open topic stays at the foot of the detail
        let composer: Element<'a, Message> = match &self.topic {
            Some(t) if self.topic_form.is_none() => container(
                row![
                    iced::widget::pick_list(
                        std::iter::once("by key".to_owned())
                            .chain((0..self.topic_partitions(t)).map(|p| format!("p{p}")))
                            .collect::<Vec<_>>(),
                        Some(match self.composer_partition {
                            Some(p) => format!("p{p}"),
                            None => "by key".to_owned(),
                        }),
                        |choice| cfg(CfgMsg::ComposerPartition(
                            choice.strip_prefix('p').and_then(|n| n.parse().ok())
                        ))
                    )
                    .text_size(12)
                    .padding([8, 10])
                    .width(130),
                    text_input(&format!("Message for #{t}, sent as you"), &self.composer)
                        .on_input(|v| cfg(CfgMsg::ComposerText(v)))
                        .on_submit(cfg(CfgMsg::ComposerSend))
                        .style(crate::theme::input_style_marked(
                            crate::ui::is_invalid(&self.notice) && self.composer.trim().is_empty()
                        ))
                        .padding([9, 12])
                        .size(12.5)
                        .width(Length::Fill),
                    crate::panel::btn_primary("Send", PanelMsg::Cfg(CfgMsg::ComposerSend)),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
            )
            .padding([0, 28])
            .width(Length::Fill)
            .height(64)
            .align_y(iced::alignment::Vertical::Center)
            .style(crate::theme::top_line_style)
            .into(),
            _ => container(text("")).height(0).into(),
        };
        row![
            crate::panel::list_box_sized(250.0, Some(top), items, vec![]),
            column![detail, composer].width(Length::Fill)
        ]
        .height(Length::Fill)
        .into()
    }

    /// The one form for a new topic (name, partitions) or a message for the open topic.
    fn topic_form_view<'a>(&'a self, form: &'a TopicForm) -> Element<'a, Message> {
        let input = |hint: &'static str, value: &'a str, on: fn(String) -> CfgMsg| {
            text_input(hint, value)
                .on_input(move |t| cfg(on(t)))
                .on_submit(cfg(CfgMsg::TopicFormSubmit))
                .style(crate::theme::input_style_marked(
                    crate::ui::is_invalid(&self.notice) && value.trim().is_empty() && hint != "1",
                ))
                .padding([8, 10])
                .size(13)
        };
        let mut col = column![].spacing(12);
        if form.publish {
            let topic = self.topic.as_deref().unwrap_or("");
            col = col
                .push(
                    text(format!("Message for #{topic}"))
                        .size(18)
                        .color(crate::theme::HI),
                )
                .push(
                    column![
                        crate::theme::label("MESSAGE"),
                        input("hello", &form.first, CfgMsg::TopicFormFirst)
                    ]
                    .spacing(6),
                );
        } else {
            col = col
                .push(text("New topic").size(18).color(crate::theme::HI))
                .push(
                    column![
                        crate::theme::label("NAME"),
                        input("news", &form.first, CfgMsg::TopicFormFirst)
                    ]
                    .spacing(6),
                )
                .push(
                    column![
                        crate::theme::label("PARTITIONS · EMPTY FOR 1"),
                        input("1", &form.second, CfgMsg::TopicFormSecond)
                    ]
                    .spacing(6),
                );
        }
        container(
            col.push(
                row![
                    cfg_btn(
                        if form.publish { "Send" } else { "Create" },
                        CfgMsg::TopicFormSubmit,
                        true
                    ),
                    cfg_btn("Cancel", CfgMsg::TopicFormCancel, false),
                ]
                .spacing(10),
            ),
        )
        .max_width(600)
        .into()
    }

    fn vbox<'a>(&'a self, label: &'static str, field: VField) -> Element<'a, Message> {
        let value = self
            .vfields
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        column![
            text(label).size(12).color(DIM),
            text_input("", value)
                .on_input(move |v| cfg(CfgMsg::VendorField(field, v)))
                .size(13)
                .padding([8, 10])
                .style(crate::theme::input_style)
                .width(Length::Fill)
        ]
        .spacing(6)
        .width(Length::Fill)
        .into()
    }

    fn rbox<'a>(&'a self, label: &'static str, field: RField) -> Element<'a, Message> {
        let value = self
            .rfields
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        row![
            iced::widget::container(text(label).size(12).color(DIM)).width(110),
            text_input("", value)
                .on_input(move |v| cfg(CfgMsg::RoleField(field, v)))
                .size(13)
                .padding([8, 10])
                .style(crate::theme::input_style)
                .width(Length::Fill)
        ]
        .spacing(8)
        .into()
    }

    pub fn models_view<'a>(&'a self) -> Element<'a, Message> {
        if self.loading && self.models.vendors.is_empty() && self.models.roles.is_empty() {
            return loading_state("Loading models and roles…");
        }
        let mut items: Vec<Element<'a, Message>> = vec![
            crate::theme::label("INSTALLED").into(),
            button(column![
                text("On this computer").size(13),
                text(if self.installed.is_empty() {
                    "looking…".to_owned()
                } else {
                    format!(
                        "{} agent(s) installed",
                        self.installed.iter().filter(|r| r.installed()).count()
                    )
                })
                .size(11)
                .color(DIM)
            ])
            .width(Length::Fill)
            .padding([4, 8])
            .style(item_style(matches!(self.editing, Some(Editing::Installed))))
            .on_press(cfg(CfgMsg::SelectInstalled))
            .into(),
            crate::theme::label("TEAM").into(),
            button(row![
                text("Roles").size(13),
                container(text("")).width(Length::Fill),
                text(self.models.roles.len().to_string())
                    .size(11)
                    .color(DIM),
            ])
            .width(Length::Fill)
            .padding([4, 8])
            .style(item_style(matches!(self.editing, Some(Editing::Roles))))
            .on_press(cfg(CfgMsg::SelectRoles))
            .into(),
            crate::theme::label("VENDORS").into(),
        ];
        for (name, v) in &self.models.vendors {
            let selected = matches!(&self.editing, Some(Editing::Vendor(n)) if n == name);
            items.push(
                button(column![
                    text(name.clone()).size(13),
                    text(v.command.clone()).size(11).color(DIM)
                ])
                .width(Length::Fill)
                .padding([4, 8])
                .style(item_style(selected))
                .on_press(crate::ui::nav(
                    selected,
                    cfg(CfgMsg::SelectVendor(name.clone())),
                ))
                .into(),
            );
        }
        items.push(crate::theme::label("ROLES").into());
        for (name, r) in &self.models.roles {
            let selected = matches!(&self.editing, Some(Editing::Role(n)) if n == name);
            items.push(
                button(column![
                    text(name.clone()).size(13),
                    text(format!("{} {}", r.vendor, r.model))
                        .size(11)
                        .color(DIM)
                ])
                .width(Length::Fill)
                .padding([4, 8])
                .style(item_style(selected))
                .on_press(crate::ui::nav(
                    selected,
                    cfg(CfgMsg::SelectRole(name.clone())),
                ))
                .into(),
            );
        }
        let footer: Vec<Element<'a, Message>> = vec![];
        let target = format!("Saved in {}", display(&self.models.target));
        let detail: Element<'a, Message> = match &self.editing {
            None => text("Pick a vendor or a role.").color(DIM).into(),
            Some(Editing::Installed) => self.installed_view(),
            Some(Editing::Roles) => self.roles_view(),
            Some(editing) => {
                let mut form = column![].spacing(6);
                match editing {
                    Editing::Vendor(_) => {
                        let val = |f: VField| {
                            self.vfields
                                .iter()
                                .find(|(x, _)| *x == f)
                                .map(|(_, v)| v.clone())
                                .unwrap_or_default()
                        };
                        let users: Vec<&str> = self
                            .models
                            .roles
                            .iter()
                            .filter(|(_, r)| r.vendor == val(VField::Name))
                            .map(|(n, _)| n.as_str())
                            .collect();
                        form = form.push(
                            row![
                                text(format!("Vendor {}", val(VField::Name)))
                                    .font(crate::theme::MONO_BOLD)
                                    .size(18)
                                    .color(crate::theme::HI),
                                text(if users.is_empty() {
                                    "used by no role".to_owned()
                                } else {
                                    format!("used by {}", users.join(", "))
                                })
                                .size(11.5)
                                .color(DIM),
                            ]
                            .spacing(12)
                            .align_y(iced::Alignment::Center),
                        );
                        form = form
                            .push(
                                row![
                                    self.vbox("Name", VField::Name),
                                    self.vbox("Command", VField::Command)
                                ]
                                .spacing(16),
                            )
                            .push(
                                row![
                                    self.vbox("Extra arguments", VField::Args),
                                    self.vbox("Model flag", VField::ModelFlag)
                                ]
                                .spacing(16),
                            )
                            .push(
                                row![
                                    self.vbox("Models (comma or space separated)", VField::Models),
                                    self.vbox("Default model", VField::DefaultModel)
                                ]
                                .spacing(16),
                            )
                            .push(self.vbox("Shepherd agent", VField::ShepherdAgent));
                        form = form.push(self.model_chips());
                        form = form.push(
                            checkbox(self.vendor.mcp.supported)
                                .label("MCP supported")
                                .on_toggle(|on| cfg(CfgMsg::VendorMcp(on)))
                                .size(14)
                                .text_size(12),
                        );
                        form = form.push(
                            checkbox(self.vendor.disabled)
                                .label("disabled")
                                .on_toggle(|on| cfg(CfgMsg::VendorDisabled(on)))
                                .size(14)
                                .text_size(12),
                        );
                        let flag = val(VField::ModelFlag);
                        let model = val(VField::DefaultModel);
                        let mut starts = format!("Starts as: {}", val(VField::Command));
                        if !val(VField::Args).is_empty() {
                            starts.push_str(&format!(" {}", val(VField::Args)));
                        }
                        if !flag.is_empty() && !model.is_empty() {
                            starts.push_str(&format!(" {flag} {model}"));
                        }
                        form = form.push(
                            container(text(starts).size(12))
                                .padding([12, 14])
                                .width(Length::Fill)
                                .style(card_style),
                        );
                    }
                    Editing::Installed | Editing::Roles => {}
                    Editing::Role(_) => {
                        form = form.push(text("ROLE").size(11).color(DIM));
                        let vendor_names: Vec<String> =
                            self.models.vendors.keys().cloned().collect();
                        form = form
                            .push(self.rbox("name", RField::Name))
                            .push(self.picked_rbox("vendor", RField::Vendor, vendor_names))
                            .push(self.picked_rbox("model", RField::Model, self.role_models()))
                            .push(self.rbox("fallback", RField::Fallback))
                            .push(self.rbox("must differ from", RField::MustDiffer))
                            .push(self.skills_row())
                            .push(self.rbox("prompt", RField::Prompt));
                        form = form.push(
                            checkbox(self.role.worktree)
                                .label("own git worktree")
                                .on_toggle(|on| cfg(CfgMsg::RoleWorktree(on)))
                                .size(14)
                                .text_size(12),
                        );
                        form = form.push(
                            checkbox(self.role.can_spawn)
                                .label("can start other agents")
                                .on_toggle(|on| cfg(CfgMsg::RoleSpawn(on)))
                                .size(14)
                                .text_size(12),
                        );
                    }
                }
                let delete_label = if self.armed_delete {
                    "Confirm: delete"
                } else {
                    "Delete"
                };
                form.push(
                    button(container(text(delete_label).size(13)).center_y(Length::Fill))
                        .padding([0, 12])
                        .height(32)
                        .style(crate::theme::outline_button_style(!self.armed_delete))
                        .on_press(cfg(CfgMsg::Delete)),
                )
                .push(text(target).size(11).color(DIM))
                .push(
                    text(if self.is_new {
                        "New entry: pick a name, then save."
                    } else {
                        "Lists are comma or space separated. Saving rewrites config.yaml (comments are lost, a .bak is kept)."
                    })
                    .size(11)
                    .color(DIM),
                )
                    .into()
            }
        };
        let _ = footer;
        // the page is as wide as its content needs (720 a vendor, 820 the roles), left aligned
        let width = if matches!(self.editing, Some(Editing::Roles)) {
            820.0
        } else {
            720.0
        };
        let detail = container(detail).max_width(width);
        let mut page = column![detail_box(detail.into())].height(Length::Fill);
        if self.dirty() {
            page = page.push(self.save_bar());
        }
        let base: Element<'a, Message> = row![
            crate::panel::list_box_sized(230.0, None, items, vec![]),
            page.width(Length::Fill)
        ]
        .height(Length::Fill)
        .into();
        match &self.skill_picker {
            Some(picker) => iced::widget::stack![base, self.skill_picker_view(picker)].into(),
            None => base,
        }
    }

    /// The bar pinned under the settings while there are changes to write: orange line on top.
    fn save_bar<'a>(&'a self) -> Element<'a, Message> {
        container(
            row![
                text("■").size(8).color(crate::theme::WARN),
                text("Unsaved changes to config.yaml.").size(12),
                container(text("")).width(Length::Fill),
                cfg_btn("Discard", CfgMsg::Discard, false),
                crate::panel::btn_primary("Save to config.yaml", PanelMsg::Cfg(CfgMsg::Save)),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
        )
        .padding([0, 24])
        .height(52)
        .width(Length::Fill)
        .align_y(iced::alignment::Vertical::Center)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x0E, 0x0B, 0x09))),
            border: iced::Border {
                color: crate::theme::WARN,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
    }

    /// Every role as a row: its name, the vendor and the model as segmented controls, and Remove.
    fn roles_view<'a>(&'a self) -> Element<'a, Message> {
        let segments =
            |options: Vec<String>, current: String, on: Box<dyn Fn(String) -> CfgMsg>| {
                let mut r = row![].spacing(4).width(Length::Fill);
                for option in options {
                    let active = option == current;
                    let message = cfg(on(option.clone()));
                    r = r.push(
                        button(text(option).size(11.5))
                            .padding([4, 10])
                            .style(crate::theme::chip_style(active))
                            .on_press(message),
                    );
                }
                r.wrap()
            };
        let mut table = column![row![
            container(text("Role").size(11).color(DIM)).width(180),
            container(text("Vendor").size(11).color(DIM)).width(Length::FillPortion(2)),
            container(text("Model").size(11).color(DIM)).width(Length::FillPortion(3)),
            container(text("")).width(70),
        ]
        .spacing(12)]
        .spacing(12);
        for name in self.models.roles.keys() {
            if self.role_removed.contains(name) {
                continue;
            }
            let role = self.role_now(name);
            let changed = self.role_pending.contains_key(name);
            let vendors: Vec<String> = self.models.vendors.keys().cloned().collect();
            let mut models = self.vendor_models(&role.vendor);
            if !role.model.is_empty() && !models.contains(&role.model) {
                models.push(role.model.clone());
            }
            let owner = name.clone();
            let owner2 = name.clone();
            table = table.push(
                row![
                    button(text(name.clone()).size(13).color(if changed {
                        crate::theme::WARN
                    } else {
                        crate::theme::HI
                    }))
                    .width(180)
                    .padding([4, 8])
                    .style(crate::theme::link_button_style())
                    .on_press(cfg(CfgMsg::SelectRole(name.clone()))),
                    container(segments(
                        vendors,
                        role.vendor.clone(),
                        Box::new(move |v| CfgMsg::RolesVendor(owner.clone(), v)),
                    ))
                    .width(Length::FillPortion(2)),
                    container(segments(
                        models,
                        role.model.clone(),
                        Box::new(move |m| CfgMsg::RolesModel(owner2.clone(), m)),
                    ))
                    .width(Length::FillPortion(3)),
                    container(
                        button(text("Remove").size(11.5).color(DIM))
                            .padding([4, 8])
                            .style(crate::theme::link_button_style())
                            .on_press(cfg(CfgMsg::RolesRemove(name.clone())))
                    )
                    .width(70),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
            );
        }
        column![
            text("Roles").font(crate::theme::MONO_BOLD).size(18).color(crate::theme::HI),
            text("The vendor and model an agent of each role starts with. A pattern or the New agent form can still override them.")
                .size(12)
                .color(DIM),
            table,
        ]
        .spacing(14)
        .into()
    }

    /// What is installed on this computer: each agent CLI with its vendor and its models.
    fn installed_view<'a>(&'a self) -> Element<'a, Message> {
        let mut rows = column![].spacing(0);
        if self.installed.is_empty() {
            rows = rows.push(text("Looking at this computer…").size(13).color(DIM));
        }
        for r in &self.installed {
            let (dot, state_text) = match (r.installed(), r.state.as_deref()) {
                (true, Some("available") | None) => (OK, "installed"),
                (true, Some(other)) => (WARN, other),
                (false, _) => (crate::theme::BAD, "not installed"),
            };
            let vendor: Element<'a, Message> = match &r.vendor {
                Some(name) => crate::panel::btn_link(
                    format!("vendor {name}"),
                    PanelMsg::Cfg(CfgMsg::SelectVendor(name.clone())),
                ),
                None => cfg_btn(
                    "Add as vendor",
                    CfgMsg::AddVendorFrom(r.command.clone()),
                    false,
                ),
            };
            let models = if r.models.is_empty() {
                "—".to_owned()
            } else {
                r.models.join(", ")
            };
            rows = rows.push(column![
                container(
                    row![
                        text("●").size(11).color(dot),
                        column![
                            text(r.command.clone()).size(13).color(crate::theme::HI),
                            text(r.path.clone().unwrap_or_else(|| state_text.to_owned()))
                                .size(11)
                                .color(DIM),
                        ]
                        .width(Length::FillPortion(3))
                        .spacing(1),
                        column![
                            text("default model").size(11).color(DIM),
                            text(r.default_model.clone().unwrap_or_else(|| "—".into())).size(13),
                        ]
                        .width(Length::FillPortion(2))
                        .spacing(1),
                        column![text("models").size(11).color(DIM), text(models).size(13)]
                            .width(Length::FillPortion(5))
                            .spacing(1),
                        vendor,
                    ]
                    .spacing(14)
                    .align_y(iced::Alignment::Center),
                )
                .padding([10, 0]),
                container(text(""))
                    .width(Length::Fill)
                    .height(1)
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(crate::theme::LINE)),
                        ..container::Style::default()
                    }),
            ]);
        }
        column![
            text("Installed on this computer")
                .size(18)
                .color(crate::theme::HI),
            text("Agent commands found on your PATH. A vendor whose command is missing cannot start agents.")
                .size(12)
                .color(DIM),
            cfg_btn("Look again", CfgMsg::LookAgain, false),
            crate::panel::card(
                "AGENTS · VENDORS · MODELS",
                Some(
                    text(format!(
                        "{} installed",
                        self.installed.iter().filter(|r| r.installed()).count()
                    ))
                    .size(11)
                    .color(DIM)
                    .into()
                ),
                rows.into(),
            ),
        ]
        .spacing(14)
        .max_width(1000)
        .into()
    }

    /// Under a vendor's `models`: the models its agent offers that are not in the list yet, one click to add.
    fn model_chips<'a>(&'a self) -> Element<'a, Message> {
        let found = self
            .installed
            .iter()
            .find(|r| !self.vendor.command.is_empty() && r.command == self.vendor.command);
        let mut chips = row![].spacing(6).align_y(iced::Alignment::Center);
        let mut any = false;
        if let Some(found) = found {
            for model in found
                .models
                .iter()
                .filter(|m| !self.vendor.models.contains(m))
            {
                any = true;
                let mut all = self.vendor.models.clone();
                all.push(model.clone());
                chips = chips.push(chip(
                    format!("+ {model}"),
                    CfgMsg::VendorField(VField::Models, all.join(", ")),
                ));
            }
            if let Some(default) = &found.default_model {
                if &self.vendor.default_model != default {
                    any = true;
                    chips = chips.push(chip(
                        format!("default → {default}"),
                        CfgMsg::VendorField(VField::DefaultModel, default.clone()),
                    ));
                }
            }
        }
        if !any {
            return text("").into();
        }
        row![
            container(text("installed").size(12).color(DIM)).width(110),
            chips.wrap()
        ]
        .spacing(8)
        .into()
    }

    /// The models a role can pick: its vendor's list plus what that vendor's agent offers on this computer.
    fn role_models(&self) -> Vec<String> {
        let vendor = self.models.vendors.get(&self.role.vendor);
        let mut models: Vec<String> = vendor.map(|v| v.models.clone()).unwrap_or_default();
        if let Some(v) = vendor {
            if let Some(found) = self.installed.iter().find(|r| r.command == v.command) {
                for m in &found.models {
                    if !models.contains(m) {
                        models.push(m.clone());
                    }
                }
            }
        }
        models
    }

    /// A text field of a role that also offers a list to pick from (the text stays editable).
    fn picked_rbox<'a>(
        &'a self,
        label: &'static str,
        field: RField,
        options: Vec<String>,
    ) -> Element<'a, Message> {
        row![
            self.rbox(label, field),
            iced::widget::pick_list(options, None::<String>, move |choice| {
                cfg(CfgMsg::RoleField(field, choice))
            })
            .placeholder("pick…")
            .width(150),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// The skills of a role: what is assigned, and the button that opens the window to change it.
    fn skills_row<'a>(&'a self) -> Element<'a, Message> {
        let assigned = self
            .rfields
            .iter()
            .find(|(f, _)| *f == RField::Skills)
            .map(|(_, v)| words(v))
            .unwrap_or_default();
        row![
            container(text("skills").size(12).color(DIM)).width(110),
            column![
                text(if assigned.is_empty() {
                    "none assigned".to_owned()
                } else {
                    assigned.join(", ")
                })
                .size(13),
                text(format!("{} assigned", assigned.len()))
                    .size(11)
                    .color(DIM),
            ]
            .width(Length::Fill)
            .spacing(1),
            cfg_btn("Assign skills…", CfgMsg::SkillPickOpen, false),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// The window that assigns skills to a role: every skill Shepherd has, ticked or not, with a filter.
    fn skill_picker_view<'a>(&'a self, picker: &'a SkillPicker) -> Element<'a, Message> {
        let role = match &self.editing {
            Some(Editing::Role(name)) => name.clone(),
            _ => String::new(),
        };
        let filter = picker.filter.to_lowercase();
        let mut list = column![].spacing(2);
        let mut shown = 0;
        for s in self.skills.iter().filter(|s| {
            filter.is_empty()
                || s.name.to_lowercase().contains(&filter)
                || s.description.to_lowercase().contains(&filter)
        }) {
            shown += 1;
            let ticked = picker.selected.contains(&s.name);
            list = list.push(
                button(
                    row![
                        checkbox(ticked).size(15),
                        column![
                            row![
                                text(s.name.clone()).size(13).color(crate::theme::HI),
                                text(s.scope.to_lowercase()).size(11).color(DIM),
                            ]
                            .spacing(8),
                            text(s.description.chars().take(110).collect::<String>())
                                .size(11)
                                .color(DIM),
                        ]
                        .spacing(1)
                        .width(Length::Fill),
                    ]
                    .spacing(12)
                    .align_y(iced::Alignment::Center),
                )
                .width(Length::Fill)
                .padding([6, 8])
                .style(item_style(ticked))
                .on_press(cfg(CfgMsg::SkillPickToggle(s.name.clone()))),
            );
        }
        // assigned to the role but unknown to the catalog: shown so that they can be taken off
        for missing in picker
            .selected
            .iter()
            .filter(|n| !self.skills.iter().any(|s| &s.name == *n))
        {
            list = list.push(
                button(
                    row![
                        checkbox(true).size(15),
                        text(missing.clone()).size(13),
                        text("not found").size(11).color(WARN),
                    ]
                    .spacing(12)
                    .align_y(iced::Alignment::Center),
                )
                .width(Length::Fill)
                .padding([6, 8])
                .style(item_style(true))
                .on_press(cfg(CfgMsg::SkillPickToggle(missing.clone()))),
            );
        }
        if shown == 0 {
            list = list.push(text("No skill matches.").size(12).color(DIM));
        }
        let dialog = container(
            column![
                row![
                    text(format!("Skills of {role}"))
                        .size(18)
                        .color(crate::theme::HI),
                    container(text("")).width(Length::Fill),
                    text(format!("{} selected", picker.selected.len()))
                        .size(12)
                        .color(ACCENT),
                ]
                .align_y(iced::Alignment::Center),
                text_input("Filter skills", &picker.filter)
                    .on_input(|f| cfg(CfgMsg::SkillPickFilter(f)))
                    .style(crate::theme::input_style)
                    .padding([8, 10])
                    .size(13),
                iced::widget::scrollable(list).height(Length::Fixed(340.0)),
                row![
                    cfg_btn("Done", CfgMsg::SkillPickDone, true),
                    cfg_btn("Cancel", CfgMsg::SkillPickCancel, false),
                    text("Done fills the role's skills; Save keeps them in config.yaml")
                        .size(11)
                        .color(DIM),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
            ]
            .spacing(12),
        )
        .padding(18)
        .width(560)
        .style(card_style);
        // `opaque` keeps clicks from reaching what is under the window
        iced::widget::opaque(
            container(dialog)
                .center(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(Color::from_rgba8(5, 5, 8, 0.7))),
                    ..container::Style::default()
                }),
        )
    }
}

fn cfg(msg: CfgMsg) -> Message {
    Message::Panel(crate::panel::PanelMsg::Cfg(msg))
}

/// A `.btn` (the primary, bright one or the outlined one) that sends a config message.
fn cfg_btn<'a>(label: &'static str, msg: CfgMsg, primary: bool) -> Element<'a, Message> {
    if primary {
        crate::panel::btn_primary(label, PanelMsg::Cfg(msg))
    } else {
        crate::panel::btn(label, PanelMsg::Cfg(msg))
    }
}

/// A small button that sends a config message (for picking a suggestion).
fn chip<'a>(label: String, msg: CfgMsg) -> Element<'a, Message> {
    button(text(label).size(12))
        .padding([4, 10])
        .style(crate::theme::outline_button_style(false))
        .on_press(cfg(msg))
        .into()
}

/// A filter chip of the config views.
fn chip_btn<'a>(label: String, on: bool, msg: CfgMsg) -> Element<'a, Message> {
    button(text(label).size(11.5))
        .padding([3, 10])
        .style(crate::theme::chip_style(on))
        .on_press(cfg(msg))
        .into()
}

fn refresh_button<'a>() -> Element<'a, Message> {
    button(container(text("Refresh").size(13)).center_y(Length::Fill))
        .padding([0, 12])
        .height(32)
        .style(crate::theme::outline_button_style(true))
        .on_press(cfg(CfgMsg::Refresh))
        .into()
}

/// `.empty`: a boxed message with what to do next.
fn empty_state<'a>(title: &'static str, detail: &'static str) -> Element<'a, Message> {
    container(
        column![
            text(title).size(16).color(crate::theme::HI),
            text(detail).size(13).color(DIM),
            refresh_button(),
        ]
        .spacing(8),
    )
    .padding(28)
    .max_width(640)
    .style(card_style)
    .into()
}

fn loading_state<'a>(label: &'static str) -> Element<'a, Message> {
    container(
        column![
            text("●").size(18).color(crate::theme::ACCENT),
            text(label).size(14),
            text("The data is being loaded in the background.")
                .size(12)
                .color(DIM),
        ]
        .spacing(8),
    )
    .padding(20)
    .style(crate::theme::card_style)
    .into()
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_split_on_commas_and_spaces_and_ignore_the_empty_bits() {
        assert_eq!(words("a, b  c,,d "), ["a", "b", "c", "d"]);
        assert!(words("  , ").is_empty());
    }

    #[test]
    fn the_git_form_checks_its_fields_before_asking_the_server() {
        let root = Path::new("/nowhere");
        let mut panel = ConfigPanel::default();
        // a submit with nothing open does nothing
        let _ = panel.update(CfgMsg::GitFormSubmit, root);
        assert!(panel.notice.is_empty());

        // a branch needs a name; the form stays open so it can be fixed
        let _ = panel.update(CfgMsg::GitFormOpen(false), root);
        let _ = panel.update(CfgMsg::GitFormSecond("main".into()), root);
        let _ = panel.update(CfgMsg::GitFormSubmit, root);
        assert_eq!(panel.notice, crate::ui::invalid("A branch needs a name."));
        assert!(panel.git_form.is_some());
        // a name is enough (the base may be empty) and closes the form
        let _ = panel.update(CfgMsg::GitFormFirst("feat/x".into()), root);
        let _ = panel.update(CfgMsg::GitFormSubmit, root);
        assert_eq!(panel.git_form, None);

        // a worktree needs a folder and a branch
        let _ = panel.update(CfgMsg::GitFormOpen(true), root);
        let _ = panel.update(CfgMsg::GitFormFirst("../wt".into()), root);
        let _ = panel.update(CfgMsg::GitFormSubmit, root);
        assert_eq!(
            panel.notice,
            crate::ui::invalid("A worktree needs a folder and a branch.")
        );
        assert!(panel.git_form.is_some());
        let _ = panel.update(CfgMsg::GitFormCancel, root);
        assert_eq!(panel.git_form, None);
    }

    #[test]
    fn removing_a_worktree_needs_two_clicks_on_the_same_one() {
        let root = Path::new("/nowhere");
        let mut panel = ConfigPanel::default();
        let _ = panel.update(CfgMsg::WorktreeRemove("/a".into()), root);
        assert_eq!(panel.armed_worktree.as_deref(), Some("/a"));
        // another worktree starts over instead of removing the first
        let _ = panel.update(CfgMsg::WorktreeRemove("/b".into()), root);
        assert_eq!(panel.armed_worktree.as_deref(), Some("/b"));
        // selecting a worktree disarms
        let _ = panel.update(CfgMsg::SelectWorktree("/a".into()), root);
        assert_eq!(panel.armed_worktree, None);
        // the second click on the same one sends the request and leaves nothing armed
        let _ = panel.update(CfgMsg::WorktreeRemove("/a".into()), root);
        let _ = panel.update(CfgMsg::WorktreeRemove("/a".into()), root);
        assert_eq!(panel.armed_worktree, None);
    }

    #[test]
    fn what_the_server_answers_becomes_the_notice() {
        let root = Path::new("/nowhere");
        let mut panel = ConfigPanel::default();
        let _ = panel.update(CfgMsg::GitDone(Ok("Branch feat/x created".into())), root);
        assert_eq!(panel.notice, "Branch feat/x created");
        let _ = panel.update(
            CfgMsg::GitDone(Err("the main checkout cannot be removed".into())),
            root,
        );
        // the summary first, then the server's words as sent
        assert_eq!(
            panel.notice,
            "Not done.\nthe main checkout cannot be removed"
        );
    }

    #[test]
    fn the_roles_table_keeps_changes_until_saved_and_discard_drops_them() {
        let root = Path::new("/nowhere");
        let mut panel = ConfigPanel::default();
        let vendor = |models: &[&str]| Vendor {
            command: "x".into(),
            models: models.iter().map(|m| (*m).to_owned()).collect(),
            ..Vendor::default()
        };
        panel
            .models
            .vendors
            .insert("a".into(), vendor(&["a1", "a2"]));
        panel.models.vendors.insert("b".into(), vendor(&["b1"]));
        panel.models.roles.insert(
            "coder".into(),
            Role {
                vendor: "a".into(),
                model: "a2".into(),
                ..Role::default()
            },
        );
        let _ = panel.update(CfgMsg::SelectRoles, root);
        assert!(!panel.dirty());
        // another vendor picks that vendor's first model
        let _ = panel.update(CfgMsg::RolesVendor("coder".into(), "b".into()), root);
        assert!(panel.dirty());
        let now = panel.role_now("coder");
        assert_eq!((now.vendor.as_str(), now.model.as_str()), ("b", "b1"));
        // what is on disk is untouched until Save
        assert_eq!(panel.models.roles["coder"].vendor, "a");
        let _ = panel.update(CfgMsg::Discard, root);
        assert!(!panel.dirty());
        assert_eq!(panel.role_now("coder").vendor, "a");
        assert!(crate::ui::Toast::new(panel.notice.clone()).info);
    }

    #[test]
    fn refresh_on_events_says_how_many_topics_it_reread() {
        let root = Path::new("/nowhere");
        let mut panel = ConfigPanel::default();
        let _ = panel.update(CfgMsg::Topics(Ok(vec![])), root);
        assert!(panel.notice.is_empty(), "a quiet reload says nothing");
        let _ = panel.update(CfgMsg::RefreshEvents, root);
        let _ = panel.update(CfgMsg::Topics(Ok(vec![])), root);
        assert!(crate::ui::Toast::new(panel.notice.clone()).info);
        assert!(panel.notice.contains("Reread 0 topics from the bus."));
    }

    #[test]
    fn the_topic_forms_check_their_fields_before_asking_the_server() {
        let root = Path::new("/p");
        let mut panel = ConfigPanel::default();
        // a message needs an open topic
        let _ = panel.update(CfgMsg::TopicFormOpen(true), root);
        assert_eq!(panel.topic_form, None);
        assert_eq!(panel.notice, "Select a topic first");

        let _ = panel.update(CfgMsg::TopicFormOpen(false), root);
        let _ = panel.update(CfgMsg::TopicFormSubmit, root);
        assert!(panel.topic_form.is_some(), "no name, nothing sent");
        let _ = panel.update(CfgMsg::TopicFormFirst("news".into()), root);
        let _ = panel.update(CfgMsg::TopicFormSecond("x".into()), root);
        let _ = panel.update(CfgMsg::TopicFormSubmit, root);
        assert!(panel.topic_form.is_some(), "partitions must be a number");
        let _ = panel.update(CfgMsg::TopicFormSecond("2".into()), root);
        let _ = panel.update(CfgMsg::TopicFormSubmit, root);
        assert_eq!(panel.topic_form, None);

        panel.topic = Some("news".into());
        let _ = panel.update(CfgMsg::TopicFormOpen(true), root);
        let _ = panel.update(CfgMsg::TopicFormSubmit, root);
        assert!(panel.topic_form.is_some(), "an empty message is not sent");
        let _ = panel.update(CfgMsg::TopicFormFirst("hi".into()), root);
        let _ = panel.update(CfgMsg::TopicFormSubmit, root);
        assert_eq!(panel.topic_form, None);

        let _ = panel.update(CfgMsg::TopicDone(Ok("Topic #news created".into())), root);
        assert_eq!(panel.notice, "Topic #news created");
    }
}
