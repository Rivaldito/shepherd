//! The pattern editor: the graph with a toolbar and an inspector. The rules (what connects to what, renaming,
//! saving) live in `shepherd_app_core::pattern_edit`; this file only holds the selection and draws.

use crate::theme::{dim, primary_button_style, surface_style};
use crate::Message;
use iced::widget::{
    button, canvas::Canvas, checkbox, column, container, row, scrollable, text, text_editor,
    text_input,
};
use iced::{Color, Element, Length};
use shepherd_app_core::pattern_edit::{AgentField, Editor, Sel};
use shepherd_app_core::pattern_graph::{relayout_keep, Graph, PatternView, NODE_H, NODE_W};
use shepherd_app_core::shepherd_core::config::{self, Config};
use shepherd_app_core::shepherd_core::patterns::{self, Severity};
use std::path::Path;

/// What the editor shows of the pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Graph,
    /// The text the pattern is saved as, to read (it cannot be typed into).
    Yaml,
}

/// The most characters of a persona (the server's limit).
const PERSONA_MAX: usize = 16_384;

/// What the dropdowns of an agent offer, from the configuration.
#[derive(Default)]
pub struct Choices {
    pub roles: Vec<String>,
    pub vendors: Vec<String>,
    /// The vendor each role runs, to list the models of the vendor an agent ends up with.
    pub role_vendor: std::collections::HashMap<String, String>,
    /// The models each vendor offers.
    pub models: std::collections::HashMap<String, Vec<String>>,
}

impl Choices {
    pub fn from_models(
        models: &shepherd_app_core::data::Models,
        models_of: impl Fn(&str) -> Vec<String>,
    ) -> Self {
        Self {
            roles: models.roles.keys().cloned().collect(),
            vendors: models.vendors.keys().cloned().collect(),
            role_vendor: models
                .roles
                .iter()
                .map(|(name, role)| (name.clone(), role.vendor.clone()))
                .collect(),
            models: models
                .vendors
                .keys()
                .map(|name| (name.clone(), models_of(name)))
                .collect(),
        }
    }

    /// The models to offer an agent: those of the vendor it picked, else of the vendor of its role.
    fn model_options(&self, vendor: &str, role: &str) -> Vec<String> {
        let vendor = if vendor.is_empty() {
            self.role_vendor.get(role).map_or("", String::as_str)
        } else {
            vendor
        };
        self.models.get(vendor).cloned().unwrap_or_default()
    }
}

/// A count of instances is 1 to 32.
fn count_options() -> Vec<String> {
    (1..=32).map(|n| n.to_string()).collect()
}

/// Levels of a hierarchy are contiguous from 0, so with `agents` nodes the deepest is `agents - 1`.
fn level_options(agents: usize) -> Vec<String> {
    (0..agents.max(1)).map(|n| n.to_string()).collect()
}

#[derive(Debug, Clone)]
pub enum EditMsg {
    Press(String),
    Move(String, f32, f32),
    AddAgent,
    AddTopic,
    ToggleConnect,
    DeleteSelected,
    Duplicate,
    Save,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    Field(Field, String),
    /// The vendor was picked from its list: the model belongs to a vendor, so a new vendor clears it.
    PickVendor(String),
    /// Something done in the persona box of the selected agent.
    Persona(text_editor::Action),
    /// Something done in the YAML view (which only reads: selecting and scrolling, never typing).
    Yaml(text_editor::Action),
    /// Switches between the graph and the read-only YAML of the pattern.
    View(ViewMode),
    Start(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    PatternName,
    Description,
    Agent(AgentField),
    TopicName,
    Partitions,
}

pub struct PatternEditor {
    pub editor: Editor,
    pub graph: Graph,
    pub selected: Option<Sel>,
    connecting: Option<Sel>,
    /// Connect is on but no sender was picked yet: the next click is the sender.
    connect_armed: bool,
    cfg: Option<Config>,
    pub problems: Vec<(bool, String, String)>,
    /// What the inspector's boxes show while typing (a half-typed number is not a valid value yet).
    pub fields: Vec<(Field, String)>,
    pub path: Option<String>,
    pub zoom: f32,
    delete_armed: bool,
    view_mode: ViewMode,
    /// The persona box of the selected agent, and the agent it holds the text of (kept while typing, so the
    /// cursor does not jump when the pattern is revalidated).
    persona: text_editor::Content,
    persona_for: Option<String>,
    /// The YAML shown in the YAML view, read when it is opened.
    yaml: text_editor::Content,
}

impl PatternEditor {
    pub fn from_view(view: PatternView) -> Self {
        let mut me = Self {
            editor: Editor::open(view.pattern, view.source),
            graph: view.graph,
            // SHEPHERD_NATIVE_SELECT=agent:<id> starts with that node selected (for pictures)
            selected: std::env::var("SHEPHERD_NATIVE_SELECT")
                .ok()
                .and_then(|k| Sel::from_key(&k)),
            connecting: None,
            connect_armed: false,
            cfg: None,
            problems: view.problems,
            fields: Vec::new(),
            path: view.path,
            zoom: 1.0,
            delete_armed: false,
            view_mode: ViewMode::Graph,
            persona: text_editor::Content::new(),
            persona_for: None,
            yaml: text_editor::Content::new(),
        };
        me.refresh_fields();
        // `SHEPHERD_NATIVE_PATTERNVIEW=yaml` opens the editor on the YAML view (for pictures)
        if std::env::var("SHEPHERD_NATIVE_PATTERNVIEW").is_ok_and(|v| v == "yaml") {
            if let Ok(yaml) = me.editor.yaml() {
                me.yaml = text_editor::Content::with_text(&yaml);
                me.view_mode = ViewMode::Yaml;
            }
        }
        me
    }

    /// Loads the persona of the selected agent into its box, when the selection moved to another agent.
    fn sync_persona(&mut self) {
        let id = match &self.selected {
            Some(Sel::Agent(id)) => Some(id.clone()),
            _ => None,
        };
        if id == self.persona_for {
            return;
        }
        let text = id
            .as_ref()
            .and_then(|id| self.editor.pattern.agents.iter().find(|a| &a.id == id))
            .and_then(|a| a.prompt.clone())
            .unwrap_or_default();
        self.persona = text_editor::Content::with_text(&text);
        self.persona_for = id;
    }

    fn refresh_fields(&mut self) {
        self.sync_persona();
        let p = &self.editor.pattern;
        self.fields = vec![
            (Field::PatternName, p.name.clone()),
            (Field::Description, p.description.clone()),
        ];
        match &self.selected {
            Some(Sel::Agent(id)) => {
                if let Some(a) = p.agents.iter().find(|a| &a.id == id) {
                    self.fields.extend([
                        (Field::Agent(AgentField::Id), a.id.clone()),
                        (Field::Agent(AgentField::Role), a.role.clone()),
                        (
                            Field::Agent(AgentField::Vendor),
                            a.vendor.clone().unwrap_or_default(),
                        ),
                        (
                            Field::Agent(AgentField::Model),
                            a.model.clone().unwrap_or_default(),
                        ),
                        (Field::Agent(AgentField::Count), a.count.to_string()),
                        (
                            Field::Agent(AgentField::Level),
                            a.level.map(|l| l.to_string()).unwrap_or_default(),
                        ),
                        (
                            Field::Agent(AgentField::Group),
                            a.group.clone().unwrap_or_default(),
                        ),
                    ]);
                }
            }
            Some(Sel::Topic(name)) => {
                if let Some(t) = p.topics.iter().find(|t| &t.name == name) {
                    self.fields.extend([
                        (Field::TopicName, t.name.clone()),
                        (Field::Partitions, t.partitions.to_string()),
                    ]);
                }
            }
            None => {}
        }
    }

    fn field(&self, field: Field) -> &str {
        self.fields
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }

    /// Re-validates after a change; the config is read once.
    fn revalidate(&mut self, root: &Path) {
        if self.cfg.is_none() {
            self.cfg = config::load_config(root).ok();
        }
        if let Some(cfg) = &self.cfg {
            self.problems = patterns::validate(&self.editor.pattern, cfg)
                .into_iter()
                .map(|p| (matches!(p.severity, Severity::Error), p.path, p.message))
                .collect();
        }
    }

    fn changed(&mut self, root: &Path) {
        self.graph = relayout_keep(&self.graph, &self.editor.pattern);
        self.revalidate(root);
    }

    /// Applies one edit; the result is a note for the status line.
    pub fn update(&mut self, msg: EditMsg, root: &Path) -> Option<String> {
        if !matches!(msg, EditMsg::DeleteSelected) {
            self.delete_armed = false;
        }
        // a built-in can be looked at, not changed
        if self.editor.read_only()
            && matches!(
                msg,
                EditMsg::AddAgent
                    | EditMsg::AddTopic
                    | EditMsg::DeleteSelected
                    | EditMsg::Start(_)
                    | EditMsg::Field(..)
            )
        {
            return Some(crate::ui::info(&format!(
                "{} is built in and read-only. Duplicate it to change it.",
                self.editor.pattern.name
            )));
        }
        match msg {
            EditMsg::Press(key) => {
                let target = Sel::from_key(&key);
                if self.connect_armed {
                    self.connect_armed = false;
                    self.connecting = target.clone();
                    self.selected = target;
                    self.refresh_fields();
                    return None;
                }
                if let (Some(from), Some(to)) = (self.connecting.take(), target.clone()) {
                    if self.editor.read_only() {
                        return Some(crate::ui::info(&format!(
                            "{} is built in and read-only. Duplicate it to change it.",
                            self.editor.pattern.name
                        )));
                    }
                    return match self.editor.connect(&from, &to) {
                        Ok(()) => {
                            self.changed(root);
                            Some(format!("Connected {} → {}", from.key(), to.key()))
                        }
                        Err(e) => {
                            let kind = match (&from, &to) {
                                (Sel::Agent(_), Sel::Agent(_)) => Some("agent"),
                                (Sel::Topic(_), Sel::Topic(_)) => Some("topic"),
                                _ => None,
                            };
                            Some(match kind {
                                Some(kind) => crate::panel::refusal(
                                    "Connect an agent to a topic, or a topic to an agent.",
                                    &format!(
                                        "error: edge {} -> {} joins two {kind}s",
                                        from.key().split_once(':').map_or("", |(_, n)| n),
                                        to.key().split_once(':').map_or("", |(_, n)| n)
                                    ),
                                ),
                                None => e,
                            })
                        }
                    };
                }
                self.selected = target;
                self.refresh_fields();
            }
            EditMsg::Move(key, x, y) => {
                if let Some(node) = self.graph.nodes.iter_mut().find(|n| n.key == key) {
                    node.x = x.max(0.0);
                    node.y = y.max(0.0);
                }
                let (w, h) = self.graph.nodes.iter().fold((0.0f32, 0.0f32), |(w, h), n| {
                    (w.max(n.x + NODE_W), h.max(n.y + NODE_H))
                });
                self.graph.width = self.graph.width.max(w + 20.0);
                self.graph.height = self.graph.height.max(h + 20.0);
            }
            EditMsg::AddAgent => {
                self.selected = Some(self.editor.add_agent());
                self.changed(root);
                self.refresh_fields();
            }
            EditMsg::AddTopic => {
                self.selected = Some(self.editor.add_topic());
                self.changed(root);
                self.refresh_fields();
            }
            EditMsg::ToggleConnect => {
                if self.connecting.is_some() || self.connect_armed {
                    self.connecting = None;
                    self.connect_armed = false;
                } else if let Some(from) = self.selected.clone() {
                    self.connecting = Some(from);
                } else {
                    self.connect_armed = true;
                }
            }
            EditMsg::DeleteSelected => {
                if self.selected.is_none() {
                    return Some("Select a node before deleting it.".into());
                }
                if !self.delete_armed {
                    self.delete_armed = true;
                    return Some("Press Delete again to remove the selected node.".into());
                }
                if let Some(sel) = self.selected.take() {
                    self.editor.delete(&sel);
                    self.changed(root);
                    self.refresh_fields();
                }
                self.delete_armed = false;
            }
            EditMsg::Duplicate => {
                let from = self.editor.pattern.name.clone();
                let name = format!("{from}-copy");
                self.editor = self.editor.duplicate(&name);
                self.path = None;
                self.refresh_fields();
                return Some(format!("Duplicated {from} as {name}. Edit it and save."));
            }
            EditMsg::ZoomIn => self.zoom = ((self.zoom * 10.0).round() / 10.0 + 0.1).min(1.4),
            EditMsg::ZoomOut => self.zoom = ((self.zoom * 10.0).round() / 10.0 - 0.1).max(0.4),
            // fit the whole graph in a viewport of about 700 x 380
            EditMsg::ResetZoom => {
                self.zoom = (700.0 / self.graph.width.max(1.0))
                    .min(380.0 / self.graph.height.max(1.0))
                    .clamp(0.4, 1.4)
            }
            EditMsg::Save => {
                let name = self.editor.pattern.name.clone();
                return Some(match self.editor.save(root) {
                    Ok(path) => {
                        self.path = Some(path.display().to_string());
                        format!("Saved {name} to .shepherd/patterns/{name}.yaml.")
                    }
                    Err(e) => {
                        crate::panel::refusal(&format!("Not saved: {name} is not valid."), &e)
                    }
                });
            }
            EditMsg::Start(on) => {
                if let Some(Sel::Agent(id)) = self.selected.clone() {
                    self.editor.set_start(&id, on);
                    self.changed(root);
                }
            }
            EditMsg::View(mode) => {
                self.view_mode = mode;
                if mode == ViewMode::Yaml {
                    match self.editor.yaml() {
                        Ok(yaml) => self.yaml = text_editor::Content::with_text(&yaml),
                        Err(e) => return Some(format!("The YAML could not be written: {e}")),
                    }
                }
            }
            EditMsg::Yaml(action) => {
                // read-only: moving, selecting and scrolling are fine, typing and deleting are not
                if !action.is_edit() {
                    self.yaml.perform(action);
                }
            }
            EditMsg::Persona(action) => {
                let Some(Sel::Agent(id)) = self.selected.clone() else {
                    return None;
                };
                if !action.is_edit() {
                    self.persona.perform(action);
                    return None;
                }
                if self.editor.read_only() {
                    return None;
                }
                self.persona.perform(action);
                let text = self.persona.text();
                let result = self.editor.set_agent_field(&id, AgentField::Prompt, &text);
                self.changed(root);
                return result.err();
            }
            EditMsg::PickVendor(vendor) => {
                if self.field(Field::Agent(AgentField::Vendor)) == vendor {
                    return None;
                }
                if let Some(error) = self.update(
                    EditMsg::Field(Field::Agent(AgentField::Vendor), vendor),
                    root,
                ) {
                    return Some(error);
                }
                return self.update(
                    EditMsg::Field(Field::Agent(AgentField::Model), String::new()),
                    root,
                );
            }
            EditMsg::Field(field, value) => {
                if let Some(slot) = self.fields.iter_mut().find(|(f, _)| *f == field) {
                    slot.1 = value.clone();
                }
                let result = match (field, self.selected.clone()) {
                    (Field::PatternName, _) if self.editor.is_new => {
                        self.editor.pattern.name = value.trim().to_owned();
                        self.editor.dirty = true;
                        Ok(())
                    }
                    (Field::PatternName, _) => {
                        Err("Only a new pattern can be renamed: duplicate it first.".to_owned())
                    }
                    (Field::Description, _) => {
                        self.editor.pattern.description = value;
                        self.editor.dirty = true;
                        Ok(())
                    }
                    (Field::Agent(f), Some(Sel::Agent(id))) => {
                        let r = self.editor.set_agent_field(&id, f, &value);
                        if f == AgentField::Id && r.is_ok() {
                            self.selected = Some(Sel::Agent(value.trim().to_owned()));
                        }
                        r
                    }
                    (Field::TopicName, Some(sel @ Sel::Topic(_))) => self
                        .editor
                        .rename(&sel, &value)
                        .map(|new| self.selected = Some(new)),
                    (Field::Partitions, Some(Sel::Topic(name))) => {
                        self.editor.set_partitions(&name, &value)
                    }
                    _ => Ok(()),
                };
                self.changed(root);
                return result.err();
            }
        }
        None
    }

    /// The persona of the selected agent: what it reads, does and publishes. In a pattern run it replaces the
    /// script of the role, so an agent without one only knows its topics.
    fn persona_box<'a>(&'a self, read_only: bool) -> Element<'a, Message> {
        let used = self.persona.text().chars().count();
        let counter =
            text(format!("{used} / {PERSONA_MAX}"))
                .size(11)
                .color(if used > PERSONA_MAX {
                    crate::theme::bad()
                } else {
                    dim()
                });
        column![
            row![
                text("persona").size(12).color(dim()).width(Length::Fill),
                counter
            ],
            text_editor(&self.persona)
                .on_action(|a| Message::Panel(crate::panel::PanelMsg::EditPattern(
                    EditMsg::Persona(a)
                )))
                .height(Length::Fixed(150.0))
                .wrapping(iced::widget::text::Wrapping::Word)
                .size(12.5)
                .padding(8)
                .style(crate::theme::editor_style),
            text(if read_only {
                "Built-in patterns are read-only."
            } else {
                "What this agent reads, does and publishes. It replaces the role's script in a run."
            })
            .size(11)
            .color(dim()),
        ]
        .spacing(4)
        .into()
    }

    /// A field of the agent chosen from a list (`empty` is the entry that leaves it unset).
    fn picker<'a>(
        &'a self,
        label: &'static str,
        field: AgentField,
        options: Vec<String>,
        empty: Option<&'static str>,
    ) -> Element<'a, Message> {
        let on_pick = move |value: String| {
            Message::Panel(crate::panel::PanelMsg::EditPattern(
                if field == AgentField::Vendor {
                    EditMsg::PickVendor(value)
                } else {
                    EditMsg::Field(Field::Agent(field), value)
                },
            ))
        };
        row![
            container(text(label).size(12).color(dim())).width(90),
            crate::config_panel::dropdown(options, self.field(Field::Agent(field)), empty, on_pick),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn labelled<'a>(&'a self, label: &'static str, field: Field) -> Element<'a, Message> {
        row![
            container(text(label).size(12).color(dim())).width(90),
            text_input("", self.field(field))
                .style(crate::theme::input_style)
                .on_input(move |v| Message::Panel(crate::panel::PanelMsg::EditPattern(
                    EditMsg::Field(field, v)
                )))
                .size(13)
                .padding([8, 10])
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Turns the pattern into an own copy called `name`, not saved yet (the template of New pattern).
    pub fn copy_as(&mut self, name: &str) {
        self.editor = self.editor.duplicate(name);
        self.path = None;
        self.refresh_fields();
        self.editor.dirty = true;
    }

    /// A blank pattern just created: it has no file yet, so there is something to save.
    pub fn mark_new(&mut self) {
        self.editor.dirty = true;
    }

    pub fn view<'a>(&'a self, delete_armed: bool, choices: &Choices) -> Element<'a, Message> {
        let edit = |m: EditMsg| Message::Panel(crate::panel::PanelMsg::EditPattern(m));
        let tool = |label: &'static str, msg: EditMsg, on: bool| {
            button(container(text(label).size(12.5)).center_y(Length::Fill))
                .padding([0, 7])
                .height(28)
                .style(crate::theme::outline_button_style(!on))
                .on_press(edit(msg))
        };
        let read_only = self.editor.read_only();
        let mut toolbar = row![
            tool("+ Agent", EditMsg::AddAgent, false),
            tool("+ Topic", EditMsg::AddTopic, false),
            tool(
                "Connect",
                EditMsg::ToggleConnect,
                self.connecting.is_some() || self.connect_armed
            ),
            tool(
                if self.delete_armed {
                    "Confirm delete"
                } else {
                    "Delete"
                },
                EditMsg::DeleteSelected,
                self.delete_armed,
            ),
        ]
        .spacing(3)
        .width(Length::Fill);
        toolbar = toolbar
            .push(container(text("")).width(1).height(20).style(|_| {
                iced::widget::container::Style {
                    background: Some(iced::Background::Color(crate::theme::line2())),
                    ..iced::widget::container::Style::default()
                }
            }))
            .push(tool("−", EditMsg::ZoomOut, false))
            .push(
                text(format!("{:.0}%", self.zoom * 100.0))
                    .size(12)
                    .color(dim()),
            )
            .push(tool("+", EditMsg::ZoomIn, false))
            .push(tool("Fit", EditMsg::ResetZoom, false));
        let state = if read_only {
            "built-in: read-only, duplicate it to change it"
        } else if self.editor.dirty {
            "● unsaved changes"
        } else {
            "saved"
        };
        let dirty = self.editor.dirty;
        let name = self.editor.pattern.name.clone();
        let can_delete = !read_only && self.path.is_some();
        let hbtn = |label: &'static str, msg: Message, style: u8, on: bool| {
            let b = button(container(text(label).size(13)).center_y(Length::Fill))
                .padding([0, 12])
                .height(32);
            let b = match style {
                0 => b.style(primary_button_style()),
                1 => b.style(crate::theme::destructive_button_style()),
                2 => b.style(crate::theme::danger_button_style()),
                _ => b.style(crate::theme::outline_button_style(false)),
            };
            if on {
                b.on_press(msg)
            } else {
                b
            }
        };
        let mode_btn = |label: &'static str, mode: ViewMode| {
            let on = self.view_mode == mode;
            button(container(text(label).size(13)).center_y(Length::Fill))
                .padding([0, 12])
                .height(32)
                .style(crate::theme::outline_button_style(!on))
                .on_press(edit(EditMsg::View(mode)))
        };
        let mut actions = row![
            mode_btn("Graph", ViewMode::Graph),
            mode_btn("YAML", ViewMode::Yaml),
            hbtn("Duplicate", edit(EditMsg::Duplicate), 3, true)
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        if !read_only {
            actions = actions.push(hbtn("Save", edit(EditMsg::Save), 3, dirty));
        }
        actions = actions.push(hbtn(
            "▶ Run this pattern",
            Message::Panel(crate::panel::PanelMsg::PickAndLaunch(name.clone())),
            0,
            true,
        ));
        if can_delete {
            actions = actions.push(hbtn(
                if delete_armed {
                    "Confirm: delete pattern"
                } else {
                    "Delete"
                },
                Message::Panel(if delete_armed {
                    crate::panel::PanelMsg::Confirm
                } else {
                    crate::panel::PanelMsg::Arm(crate::panel::Armed::DeletePattern(name.clone()))
                }),
                if delete_armed { 2 } else { 1 },
                true,
            ));
        }
        let valid_badge: Element<'a, Message> = if self.problems.iter().any(|(e, _, _)| *e) {
            crate::panel::badge("invalid", crate::theme::bad())
        } else {
            crate::panel::badge("valid", crate::theme::ok())
        };
        let mut col = column![row![
            column![
                row![
                    text(name.clone())
                        .font(crate::theme::MONO_BOLD)
                        .size(20)
                        .color(crate::theme::hi()),
                    valid_badge,
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                text(format!("{:?}, {state}", self.editor.pattern.kind))
                    .size(12)
                    .color(dim()),
            ]
            .spacing(4)
            .width(Length::Fill),
            actions.wrap(),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),]
        .spacing(10);
        if read_only {
            col = col.push(
                container(
                    row![
                        text("Built-in patterns are read-only. Duplicate this one to change it; the copy is saved in this project.")
                            .size(12)
                            .width(Length::Fill),
                        button(text("Duplicate to edit").size(11.5))
                            .padding([4, 10])
                            .style(crate::theme::outline_button_style(false))
                            .on_press(edit(EditMsg::Duplicate)),
                    ]
                    .spacing(12)
                    .align_y(iced::Alignment::Center),
                )
                .padding([8, 14])
                .width(Length::Fill)
                .style(|_| iced::widget::container::Style {
                    background: Some(iced::Background::Color(Color {
                        a: 0.08,
                        ..crate::theme::violet()
                    })),
                    border: iced::Border {
                        color: Color {
                            a: 0.38,
                            ..crate::theme::violet()
                        },
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..iced::widget::container::Style::default()
                }),
            );
        }
        if let Some(path) = &self.path {
            col = col.push(text(path.clone()).size(11).color(dim()));
        }
        let mut checks = row![].spacing(14).align_y(iced::Alignment::Center);
        if self.problems.is_empty() {
            checks = checks
                .push(text("Valid").size(12).color(crate::theme::ok()))
                .push(
                    text("■ every topic has a publisher and a reader")
                        .size(11)
                        .color(dim()),
                )
                .push(text("■ every agent is connected").size(11).color(dim()));
        }
        for (is_error, place, message) in &self.problems {
            checks = checks.push(
                text(format!(
                    "{} {place}: {message}",
                    if *is_error { "✖" } else { "▲" }
                ))
                .size(12)
                .color(if *is_error {
                    crate::theme::bad()
                } else {
                    crate::theme::warn()
                }),
            );
        }
        // inspector
        let mini_zoom = (170.0 / self.graph.width.max(1.0))
            .min(86.0 / self.graph.height.max(1.0))
            .clamp(0.05, 0.32);
        let mini_map = container(
            Canvas::new(crate::graph_view::GraphView {
                graph: &self.graph,
                selected: self.selected.as_ref().map(Sel::key),
                connecting: self.connecting.as_ref().map(Sel::key),
                zoom: mini_zoom,
                mini: true,
            })
            .width(170)
            .height(Length::Fixed(86.0)),
        )
        .width(170)
        .height(Length::Fixed(86.0))
        .style(surface_style);
        let mut inspector = column![].spacing(6);
        match &self.selected {
            Some(Sel::Agent(id)) => {
                inspector = inspector
                    .push(
                        text(format!("Agent {id}"))
                            .font(crate::theme::MONO_BOLD)
                            .size(14)
                            .color(crate::theme::hi()),
                    )
                    .push(
                        text("Changes are kept until you save the pattern.")
                            .size(11)
                            .color(dim()),
                    );
                // what the node reads and writes, from its arrows (edit them with Connect)
                let key = format!("agent:{id}");
                let index = self.graph.nodes.iter().position(|n| n.key == key);
                let topics = |reads: bool| -> String {
                    self.graph
                        .edges
                        .iter()
                        .filter(|edge| Some(if reads { edge.to } else { edge.from }) == index)
                        .map(|edge| &self.graph.nodes[if reads { edge.from } else { edge.to }])
                        .filter(|n| n.kind == shepherd_app_core::pattern_graph::NodeKind::Topic)
                        .map(|n| n.label.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                inspector = inspector
                    .push(self.labelled("node", Field::Agent(AgentField::Id)))
                    .push(self.picker("role", AgentField::Role, choices.roles.clone(), None))
                    .push(self.picker("replicas", AgentField::Count, count_options(), None))
                    .push(readonly("reads from", topics(true)))
                    .push(readonly("publishes to", topics(false)));
                let models = choices.model_options(
                    self.field(Field::Agent(AgentField::Vendor)),
                    self.field(Field::Agent(AgentField::Role)),
                );
                inspector = inspector
                    .push(self.picker(
                        "vendor",
                        AgentField::Vendor,
                        choices.vendors.clone(),
                        Some("(from role)"),
                    ))
                    .push(self.picker("model", AgentField::Model, models, Some("(default)")))
                    .push(self.picker(
                        "level",
                        AgentField::Level,
                        level_options(self.editor.pattern.agents.len()),
                        Some("(none)"),
                    ))
                    .push(self.labelled("group", Field::Agent(AgentField::Group)))
                    .push(self.persona_box(read_only));
                let start = self
                    .editor
                    .pattern
                    .agents
                    .iter()
                    .find(|a| &a.id == id)
                    .map(|a| a.start)
                    .unwrap_or(false);
                inspector = inspector.push(
                    checkbox(start)
                        .label("starts the run")
                        .on_toggle(move |on| edit(EditMsg::Start(on)))
                        .size(14)
                        .text_size(12),
                );
            }
            Some(Sel::Topic(name)) => {
                inspector = inspector
                    .push(
                        text(format!("Topic #{name}"))
                            .font(crate::theme::MONO_BOLD)
                            .size(14)
                            .color(crate::theme::hi()),
                    )
                    .push(
                        text("Changes are kept until you save the pattern.")
                            .size(11)
                            .color(dim()),
                    );
                inspector = inspector
                    .push(self.labelled("name", Field::TopicName))
                    .push(self.labelled("partitions", Field::Partitions));
            }
            None => {}
        }
        inspector = inspector
            .push(self.labelled("pattern", Field::PatternName))
            .push(self.labelled("description", Field::Description))
            .push(
                text("Click a node to edit it. Drag nodes to arrange them.")
                    .size(11)
                    .color(dim()),
            );
        let canvas = container(
            crate::theme::scroll(column![
                // room under the floating toolbar, so the first nodes are not hidden by it
                container(text("")).height(44),
                Canvas::new(crate::graph_view::GraphView {
                    graph: &self.graph,
                    selected: self.selected.as_ref().map(Sel::key),
                    connecting: self.connecting.as_ref().map(Sel::key),
                    zoom: self.zoom,
                    mini: false,
                })
                .width(Length::Fixed(self.graph.width.max(960.0) * self.zoom))
                .height(Length::Fixed(self.graph.height.max(440.0) * self.zoom)),
            ])
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::default(),
                horizontal: scrollable::Scrollbar::default(),
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(8)
        .style(crate::theme::graph_style);
        let inspector = container(
            crate::theme::scroll(
                container(inspector).padding(crate::theme::pad(0.0, 14.0, 0.0, 0.0)),
            )
            .height(Length::Fill),
        )
        .width(280)
        .height(Length::Fill)
        .padding(12)
        .style(crate::theme::inspector_style);
        // the toolbar floats over the top left of the graph and the minimap over the bottom right
        let hint = if self.connect_armed {
            "Click the node that sends, then the one that receives."
        } else if self.connecting.is_some() {
            "Now click the target. Press Connect again to stop."
        } else {
            ""
        };
        let mut floating = column![container(
            toolbar
                .width(Length::Fill)
                .align_y(iced::Alignment::Center)
                .wrap()
        )
        .padding([4, 4])
        .width(Length::Fill)
        .style(crate::theme::toast_style(crate::theme::line2()))]
        .spacing(6);
        if !hint.is_empty() {
            floating = floating.push(text(hint).size(11.5).color(crate::theme::accent()));
        }
        let viewport = iced::widget::stack![
            canvas,
            container(floating).padding(10),
            container(mini_map)
                .padding(14)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Bottom),
        ]
        .width(Length::Fill)
        .height(Length::Fill);
        if self.view_mode == ViewMode::Yaml {
            let note = if self.editor.dirty {
                "Read-only. This is the YAML the pattern is saved as, with your changes not saved yet. Edit the pattern in the graph."
            } else {
                "Read-only. This is the YAML the pattern is saved as. Edit the pattern in the graph."
            };
            col = col.push(text(note).size(11.5).color(dim()));
            col = col.push(
                container(crate::editor::view(&self.yaml, |a| {
                    Message::Panel(crate::panel::PanelMsg::EditPattern(EditMsg::Yaml(a)))
                }))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(crate::theme::graph_style),
            );
        } else {
            col = col.push(row![viewport, inspector].spacing(14).height(Length::Fill));
        }
        col = col.push(container(checks).padding([6, 0]));
        col.height(Length::Fill).into()
    }
}

/// A label with a read-only value shown like an input.
fn readonly<'a>(label: &'static str, value: String) -> Element<'a, Message> {
    row![
        container(text(label).size(12).color(dim())).width(90),
        container(
            text(if value.is_empty() {
                "—".to_owned()
            } else {
                value
            })
            .size(13)
        )
        .padding([8, 10])
        .width(Length::Fill)
        .style(crate::theme::field_style),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin() -> PatternEditor {
        let view = shepherd_app_core::pattern_graph::load(Path::new("/nowhere"), "blackboard")
            .expect("the built-in patterns ship with the app");
        PatternEditor::from_view(view)
    }

    #[test]
    fn the_models_offered_are_those_of_the_picked_vendor_else_of_the_role() {
        let mut choices = Choices::default();
        choices.role_vendor.insert("coder".into(), "codex".into());
        choices.models.insert("codex".into(), vec!["gpt".into()]);
        choices
            .models
            .insert("claude".into(), vec!["opus".into(), "haiku".into()]);
        assert_eq!(choices.model_options("claude", "coder"), ["opus", "haiku"]);
        assert_eq!(choices.model_options("", "coder"), ["gpt"]);
        assert!(choices.model_options("", "unknown").is_empty());
    }

    fn type_text(e: &mut PatternEditor, text: &str) {
        let root = Path::new("/nowhere");
        for ch in text.chars() {
            let _ = e.update(
                EditMsg::Persona(text_editor::Action::Edit(text_editor::Edit::Insert(ch))),
                root,
            );
        }
    }

    #[test]
    fn typing_a_persona_extends_the_one_the_agent_has_and_each_agent_keeps_its_own() {
        let root = Path::new("/nowhere");
        let mut e = builtin();
        e.copy_as("mine");
        let own = |e: &PatternEditor, i: usize| {
            e.editor.pattern.agents[i]
                .prompt
                .clone()
                .unwrap_or_default()
        };
        let (first, second) = (
            e.editor.pattern.agents[0].id.clone(),
            e.editor.pattern.agents[1].id.clone(),
        );
        let (old_first, old_second) = (own(&e, 0), own(&e, 1));
        assert_ne!(old_first, old_second);
        let _ = e.update(EditMsg::Press(format!("agent:{first}")), root);
        assert_eq!(
            e.persona.text().trim_end(),
            old_first,
            "the box opens with the agent's persona"
        );
        type_text(&mut e, "!");
        assert_eq!(
            own(&e, 0),
            format!("!{old_first}"),
            "typing is saved on the selected agent"
        );
        assert_eq!(own(&e, 1), old_second, "the others are not touched");
        assert!(
            e.persona.text().starts_with('!'),
            "typing does not reset the box"
        );
        let _ = e.update(EditMsg::Press(format!("agent:{second}")), root);
        assert_eq!(e.persona.text().trim_end(), old_second);
        let _ = e.update(EditMsg::Press(format!("agent:{first}")), root);
        assert!(
            e.persona.text().starts_with('!'),
            "coming back shows what was typed"
        );
    }

    #[test]
    fn a_persona_can_be_emptied_and_an_agent_without_one_starts_from_nothing() {
        let root = Path::new("/nowhere");
        let mut e = builtin();
        e.copy_as("mine");
        let _ = e.update(EditMsg::AddAgent, root);
        let Some(Sel::Agent(id)) = e.selected.clone() else {
            panic!("a new agent is selected");
        };
        assert_eq!(
            e.persona.text().trim_end(),
            "",
            "a new agent has no persona"
        );
        type_text(&mut e, "Do X");
        let agent = |e: &PatternEditor| {
            e.editor
                .pattern
                .agents
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .prompt
                .clone()
        };
        assert_eq!(agent(&e).as_deref(), Some("Do X"));
        e.editor
            .set_agent_field(&id, AgentField::Prompt, "")
            .unwrap();
        assert_eq!(agent(&e), None, "an empty persona is no persona");
    }

    #[test]
    fn a_built_in_pattern_does_not_take_a_persona() {
        let root = Path::new("/nowhere");
        let mut e = builtin();
        let id = e.editor.pattern.agents[0].id.clone();
        let before = e.editor.pattern.agents[0].prompt.clone();
        let _ = e.update(EditMsg::Press(format!("agent:{id}")), root);
        type_text(&mut e, "nope");
        assert_eq!(e.editor.pattern.agents[0].prompt, before);
    }

    #[test]
    fn the_yaml_view_shows_the_saved_text_and_cannot_be_typed_into() {
        let root = Path::new("/nowhere");
        let mut e = builtin();
        e.copy_as("mine");
        assert_eq!(e.view_mode, ViewMode::Graph);
        let _ = e.update(EditMsg::View(ViewMode::Yaml), root);
        assert_eq!(e.view_mode, ViewMode::Yaml);
        let shown = e.yaml.text();
        assert_eq!(shown.trim_end(), e.editor.yaml().unwrap().trim_end());
        let _ = e.update(
            EditMsg::Yaml(text_editor::Action::Edit(text_editor::Edit::Insert('x'))),
            root,
        );
        let _ = e.update(
            EditMsg::Yaml(text_editor::Action::Edit(text_editor::Edit::Backspace)),
            root,
        );
        assert_eq!(
            e.yaml.text(),
            shown,
            "typing and deleting do nothing in the YAML view"
        );
        assert_eq!(
            e.editor.yaml().unwrap().trim_end(),
            shown.trim_end(),
            "the pattern is untouched"
        );
        let _ = e.update(EditMsg::View(ViewMode::Graph), root);
        assert_eq!(e.view_mode, ViewMode::Graph);
    }

    #[test]
    fn counts_go_to_32_and_levels_to_one_less_than_the_agents() {
        assert_eq!(count_options().len(), 32);
        assert_eq!(count_options()[0], "1");
        assert_eq!(level_options(3), ["0", "1", "2"]);
        assert_eq!(
            level_options(0),
            ["0"],
            "an empty pattern still offers the root"
        );
    }

    #[test]
    fn a_new_vendor_clears_the_model_and_the_same_one_keeps_it() {
        let root = Path::new("/nowhere");
        let mut e = builtin();
        e.copy_as("mine");
        let id = e.editor.pattern.agents[0].id.clone();
        e.selected = Some(Sel::Agent(id.clone()));
        e.refresh_fields();
        let agent = |e: &PatternEditor| e.editor.pattern.agents[0].clone();
        let _ = e.update(
            EditMsg::Field(Field::Agent(AgentField::Vendor), "claude".into()),
            root,
        );
        let _ = e.update(
            EditMsg::Field(Field::Agent(AgentField::Model), "opus".into()),
            root,
        );
        let _ = e.update(EditMsg::PickVendor("claude".into()), root);
        assert_eq!(
            agent(&e).model.as_deref(),
            Some("opus"),
            "the same vendor keeps the model"
        );
        let _ = e.update(EditMsg::PickVendor("codex".into()), root);
        assert_eq!(agent(&e).vendor.as_deref(), Some("codex"));
        assert_eq!(
            agent(&e).model,
            None,
            "another vendor does not offer that model"
        );
    }

    #[test]
    fn zoom_stays_between_40_and_140_percent_in_steps_of_ten() {
        let root = Path::new("/nowhere");
        let mut e = builtin();
        for _ in 0..20 {
            let _ = e.update(EditMsg::ZoomIn, root);
        }
        assert!((e.zoom - 1.4).abs() < 0.001);
        for _ in 0..20 {
            let _ = e.update(EditMsg::ZoomOut, root);
        }
        assert!((e.zoom - 0.4).abs() < 0.001);
        let _ = e.update(EditMsg::ResetZoom, root);
        assert!((0.4..=1.4).contains(&e.zoom));
    }

    #[test]
    fn a_built_in_refuses_edits_with_an_info_and_two_agents_cannot_be_connected() {
        let root = Path::new("/nowhere");
        let mut e = builtin();
        let said = e.update(EditMsg::AddAgent, root).unwrap();
        assert!(said.contains("is built in and read-only"), "{said}");
        assert!(crate::ui::Toast::new(said).info);
        // Connect with nothing selected waits for the sender, then the target
        let agents: Vec<String> = e
            .graph
            .nodes
            .iter()
            .filter(|n| n.key.starts_with("agent:"))
            .map(|n| n.key.clone())
            .collect();
        assert!(agents.len() >= 2);
        let _ = e.update(EditMsg::ToggleConnect, root);
        assert!(e.connect_armed);
        let _ = e.update(EditMsg::Press(agents[0].clone()), root);
        assert!(e.connecting.is_some() && !e.connect_armed);
    }
}
