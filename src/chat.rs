//! The chat: messages, the conversation with `claude`, and the chat panel UI.

use std::path::PathBuf;

use egui::{Align, Color32, ComboBox, Frame, Id, Key, Layout, Margin, Modifiers, RichText, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::claude::{Event, Request, Run};
use crate::history::{LastChat, Message, Role};
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

pub struct Chat {
    messages: Vec<Message>,
    input: String,
    session_id: Option<String>,
    /// Working directory of the session. `--resume` only finds the session there.
    session_dir: Option<PathBuf>,
    run: Option<Run>,
    cache: CommonMarkCache,
    /// The stored last chat, while the start question is open.
    question: Option<LastChat>,
    focus_input: bool,
}

impl Chat {
    /// Creates the chat. If a last chat for the current working directory exists,
    /// the chat asks whether to continue it.
    pub fn new(cfg: &Config) -> Self {
        let question = LastChat::load().filter(|lc| lc.working_dir == cfg.working_dir());
        Self {
            messages: Vec::new(),
            input: String::new(),
            session_id: None,
            session_dir: None,
            run: None,
            cache: CommonMarkCache::default(),
            question,
            focus_input: true,
        }
    }

    pub fn has_question(&self) -> bool {
        self.question.is_some()
    }

    pub fn is_running(&self) -> bool {
        self.run.is_some()
    }

    pub fn focus_input(&mut self) {
        self.focus_input = true;
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
                Event::ToolUse(name) => self
                    .messages
                    .push(Message::new(Role::Note, format!("Tool: {name}"))),
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
        let Some(id) = &self.session_id else { return };
        LastChat {
            session_id: id.clone(),
            working_dir: self.session_dir.clone(),
            model: cfg.model.clone(),
            updated: chrono::Local::now().timestamp(),
            messages: self.messages.clone(),
        }
        .save();
    }

    fn send(&mut self, cfg: &Config, ctx: &egui::Context) {
        let prompt = self.input.trim().to_owned();
        if prompt.is_empty() || self.run.is_some() {
            return;
        }
        self.question = None;
        let dir = cfg.working_dir();
        if self.session_id.is_some() && self.session_dir != dir {
            self.messages.push(Message::new(
                Role::Note,
                "The working directory changed. This prompt starts a new conversation.",
            ));
            self.session_id = None;
        }
        self.session_dir = dir.clone();
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
        self.session_id = None;
        self.session_dir = None;
        self.question = None;
        self.focus_input = true;
        LastChat::delete();
    }

    fn continue_last(&mut self) {
        if let Some(lc) = self.question.take() {
            self.messages = lc.messages;
            self.session_id = Some(lc.session_id);
            self.session_dir = lc.working_dir;
            self.focus_input = true;
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
                && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
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
                    let dir = cfg
                        .working_dir()
                        .map(|d| d.display().to_string())
                        .unwrap_or_default();
                    ui.label(RichText::new(dir).weak().small());
                });
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if let Some(lc) = &self.question {
                        let text = format!(
                            "Continue the last chat from {} ({} messages)?",
                            lc.updated_text(),
                            lc.messages.len()
                        );
                        ui.add_space(8.0);
                        ui.label(text);
                        ui.horizontal(|ui| {
                            if ui.button("Continue").clicked() {
                                self.continue_last();
                            }
                            if ui.button("New chat").clicked() {
                                self.new_chat();
                            }
                        });
                        return;
                    }
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
                                    CommonMarkViewer::new().show(ui, &mut self.cache, &m.text);
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
