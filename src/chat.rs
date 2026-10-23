//! The chat: messages, the conversation with `claude`, and the chat panel UI.

use std::path::PathBuf;

use egui::{Align, Color32, ComboBox, Frame, Id, Key, Layout, Margin, Modifiers, RichText, Ui};
use egui_commonmark::CommonMarkCache;

use crate::claude::{Event, Request, Run};
use crate::history::{Message, Role, SavedChat};
use crate::settings::Config;

const INPUT_ID: &str = "prompt_input";

/// What the app must do after a frame of the chat.
#[derive(Default)]
pub struct ChatOutput {
    pub close_panel: bool,
    pub open_settings: bool,
}

/// The end of a turn, for the animation of the pal.
pub enum TurnEnd {
    Success,
    Failure,
}

/// A tool event of the running turn, for the animation of the pal.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolActivity {
    Use(String),
    Error,
}

/// The open history list of the chat panel.
struct History {
    chats: Vec<SavedChat>,
    query: String,
}

pub struct Chat {
    messages: Vec<Message>,
    input: String,
    /// The id of the stored chat, from the first prompt of the chat on.
    chat_id: Option<String>,
    session_id: Option<String>,
    /// Working directory of the session. `--resume` only finds the session there.
    session_dir: Option<PathBuf>,
    run: Option<Run>,
    /// Tool events since the last `take_tool_activity`.
    tool_activity: Vec<ToolActivity>,
    cache: CommonMarkCache,
    history: Option<History>,
    focus_input: bool,
}

impl Chat {
    /// Creates an empty chat. Stored chats open from the history.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            chat_id: None,
            session_id: None,
            session_dir: None,
            run: None,
            tool_activity: Vec::new(),
            cache: CommonMarkCache::default(),
            history: None,
            focus_input: true,
        }
    }

    pub fn is_running(&self) -> bool {
        self.run.is_some()
    }

    pub fn focus_input(&mut self) {
        self.focus_input = true;
    }

    /// The tool events since the last call, oldest first.
    pub fn take_tool_activity(&mut self) -> Vec<ToolActivity> {
        std::mem::take(&mut self.tool_activity)
    }

    /// Handles the events of the running prompt. Returns the end of the turn, if it ended.
    pub fn poll(&mut self, cfg: &Config) -> Option<TurnEnd> {
        let run = self.run.as_ref()?;
        let mut end = None;
        while let Some(ev) = run.try_recv() {
            match ev {
                Event::Session(id) => {
                    // Save at once, so that the session survives a quit during the turn.
                    self.session_id = Some(id);
                    self.save(cfg);
                }
                Event::Text(t) => match self.messages.last_mut() {
                    Some(m) if m.role == Role::Assistant => m.text.push_str(&t),
                    _ if t.trim().is_empty() => {}
                    _ => self.messages.push(Message::new(Role::Assistant, t)),
                },
                Event::ToolUse(name) => {
                    self.messages
                        .push(Message::new(Role::Note, format!("Tool: {name}")));
                    self.tool_activity.push(ToolActivity::Use(name));
                }
                Event::ToolError => self.tool_activity.push(ToolActivity::Error),
                Event::Finished { error } => {
                    end = Some(match error {
                        Some(e) => {
                            self.messages.push(Message::new(Role::Error, e));
                            TurnEnd::Failure
                        }
                        None => TurnEnd::Success,
                    });
                    break;
                }
            }
        }
        if end.is_some() {
            self.run = None;
            self.save(cfg);
        }
        end
    }

    fn save(&self, cfg: &Config) {
        let (Some(id), Some(session)) = (&self.chat_id, &self.session_id) else {
            return;
        };
        let _ = SavedChat {
            id: id.clone(),
            session_id: session.clone(),
            working_dir: self.session_dir.clone(),
            model: cfg.model.clone(),
            updated: chrono::Local::now().timestamp(),
            messages: self.messages.clone(),
        }
        .save();
    }

    /// The working directory of the next prompt: the directory of the
    /// session, or the configured directory for a new conversation.
    fn prompt_dir(&self, cfg: &Config) -> Option<PathBuf> {
        if self.session_id.is_some() {
            self.session_dir.clone()
        } else {
            cfg.working_dir()
        }
    }

    fn send(&mut self, cfg: &Config, ctx: &egui::Context) {
        let prompt = self.input.trim().to_owned();
        if prompt.is_empty() || self.run.is_some() {
            return;
        }
        self.history = None;
        let dir = self.prompt_dir(cfg);
        self.session_dir = dir.clone();
        if self.chat_id.is_none() {
            self.chat_id = Some(SavedChat::new_id());
        }
        self.messages.push(Message::new(Role::User, prompt.clone()));
        self.input.clear();
        let req = Request {
            program: cfg.claude_path.trim().to_owned(),
            working_dir: dir,
            model: cfg.model_arg(),
            extra_args: cfg.permission_args(),
            resume: self.session_id.clone(),
            prompt,
        };
        match Run::start(req, ctx.clone()) {
            Ok(run) => self.run = Some(run),
            Err(e) => self.messages.push(Message::new(Role::Error, e)),
        }
    }

    /// Stops the running prompt, if any.
    pub fn stop(&mut self) {
        if let Some(run) = &self.run {
            run.stop();
        }
    }

    fn new_chat(&mut self) {
        self.run = None;
        self.messages.clear();
        self.chat_id = None;
        self.session_id = None;
        self.session_dir = None;
        self.history = None;
        self.focus_input = true;
    }

    /// Opens a stored chat. The next prompt continues its conversation.
    fn open(&mut self, chat: SavedChat) {
        self.run = None;
        self.messages = chat.messages;
        self.chat_id = Some(chat.id);
        self.session_id = Some(chat.session_id);
        self.session_dir = chat.working_dir;
        self.history = None;
        self.focus_input = true;
    }

    fn toggle_history(&mut self) {
        self.history = match self.history {
            Some(_) => None,
            None => Some(History {
                chats: SavedChat::list(),
                query: String::new(),
            }),
        };
    }

    /// The list of the stored chats, with a search field. A click on a chat
    /// opens it.
    fn history_ui(&mut self, ui: &mut Ui) {
        let Some(h) = &mut self.history else { return };
        let running = self.run.is_some();
        let mut open = None;
        let mut delete = None;
        ui.add_space(8.0);
        ui.add(
            egui::TextEdit::singleline(&mut h.query)
                .hint_text("Search")
                .desired_width(f32::INFINITY),
        );
        if h.chats.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new("No stored chats.").weak());
        }
        for (i, c) in h.chats.iter().enumerate() {
            if !c.matches(&h.query) {
                continue;
            }
            ui.add_space(6.0);
            let current = self.chat_id.as_ref() == Some(&c.id);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    if ui.small_button("×").on_hover_text("Delete").clicked() {
                        delete = Some(i);
                    }
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        let title = ui
                            .add_enabled(
                                !running,
                                egui::Button::selectable(
                                    current,
                                    RichText::new(c.title()).strong(),
                                )
                                .truncate(),
                            )
                            .on_hover_text("Open and continue this chat")
                            .on_disabled_hover_text("Stop the running prompt first");
                        if title.clicked() {
                            open = Some(i);
                        }
                        let dir = c
                            .working_dir
                            .as_ref()
                            .map(|d| d.display().to_string())
                            .unwrap_or_default();
                        ui.label(
                            RichText::new(format!(
                                "{} · {} messages · {dir}",
                                c.updated_text(),
                                c.messages.len()
                            ))
                            .weak()
                            .small(),
                        );
                    });
                });
            });
        }
        if let Some(i) = delete {
            let c = h.chats.remove(i);
            SavedChat::delete(&c.id);
            if self.chat_id.as_ref() == Some(&c.id) {
                self.chat_id = Some(SavedChat::new_id());
            }
        } else if let Some(i) = open {
            let c = h.chats.remove(i);
            self.open(c);
        }
    }

    /// The chat panel.
    pub fn ui(&mut self, ui: &mut Ui, cfg: &mut Config) -> ChatOutput {
        let mut out = ChatOutput::default();
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            out.close_panel = true;
        }

        egui::Panel::top("chat_header").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("pixelpal").strong());
                ComboBox::from_id_salt("model")
                    .selected_text(&cfg.model)
                    .show_ui(ui, |ui| {
                        for m in cfg.models.clone() {
                            ui.selectable_value(&mut cfg.model, m.clone(), m);
                        }
                    })
                    .response
                    .on_hover_text("Model for the next prompt");
                if ui.button("New chat").clicked() {
                    self.new_chat();
                }
                if ui
                    .selectable_label(self.history.is_some(), "History")
                    .on_hover_text("Stored chats")
                    .clicked()
                {
                    self.toggle_history();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("×").on_hover_text("Close (Esc)").clicked() {
                        out.close_panel = true;
                    }
                    if ui.button("⚙").on_hover_text("Settings").clicked() {
                        out.open_settings = true;
                    }
                });
            });
            ui.add_space(2.0);
        });

        egui::Panel::bottom("chat_input").show(ui, |ui| {
            ui.add_space(6.0);
            let id = Id::new(INPUT_ID);
            let enter = ui.memory(|m| m.has_focus(id))
                && ui.input_mut(|i| {
                    // `consume_key` ignores extra Shift, so check it explicitly to
                    // leave Shift+Enter to the TextEdit as a newline.
                    !i.modifiers.shift && i.consume_key(Modifiers::NONE, Key::Enter)
                });
            let edit = ui.add(
                egui::TextEdit::multiline(&mut self.input)
                    .id(id)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("Prompt (Enter: send, Shift+Enter: new line)"),
            );
            if self.focus_input {
                edit.request_focus();
                self.focus_input = false;
            }
            ui.horizontal(|ui| {
                if self.run.is_some() {
                    if ui.button("Stop").clicked() {
                        self.stop();
                    }
                    ui.spinner();
                } else {
                    let can_send = !self.input.trim().is_empty();
                    if (ui
                        .add_enabled(can_send, egui::Button::new("Send"))
                        .clicked()
                        || enter)
                        && can_send
                    {
                        self.send(cfg, &ctx);
                        self.focus_input = true;
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let dir = self
                        .prompt_dir(cfg)
                        .map(|d| d.display().to_string())
                        .unwrap_or_default();
                    ui.label(RichText::new(dir).weak().small());
                });
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if self.history.is_some() {
                egui::ScrollArea::vertical()
                    .id_salt("history")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.history_ui(ui));
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if self.messages.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Ask me something.").weak());
                    }
                    for (i, m) in self.messages.iter().enumerate() {
                        ui.add_space(6.0);
                        match m.role {
                            Role::User => {
                                Frame::new()
                                    .fill(ui.visuals().faint_bg_color)
                                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                                    .corner_radius(6.0)
                                    .inner_margin(Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.add(egui::Label::new(&m.text).selectable(true));
                                    });
                            }
                            Role::Assistant => {
                                ui.push_id(i, |ui| {
                                    crate::markdown::show(ui, &mut self.cache, &m.text);
                                });
                            }
                            Role::Note => {
                                ui.label(RichText::new(&m.text).italics().weak().small());
                            }
                            Role::Error => {
                                ui.label(
                                    RichText::new(&m.text).color(Color32::from_rgb(220, 70, 70)),
                                );
                            }
                        }
                    }
                    if self.run.is_some()
                        && self
                            .messages
                            .last()
                            .is_some_and(|m| m.role != Role::Assistant)
                    {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Claude is thinking…").weak());
                        });
                    }
                    ui.add_space(6.0);
                });
        });
        out
    }
}
