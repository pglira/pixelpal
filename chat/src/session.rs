//! The chat: the messages, the conversation with `claude`, the images for the
//! next prompt, and the stored chats.

use std::path::PathBuf;

use pixelpal_core::claude::{Event, Request, Run};
use pixelpal_core::config::Config;
use pixelpal_core::history::{Message, Role, SavedChat};
use pixelpal_core::ipc::ToolActivity;

/// What changed in the chat during `Session::poll`.
#[derive(Debug, Default)]
pub struct Changes {
    /// Indices of new or changed messages.
    pub messages: Vec<usize>,
    pub tools: Vec<ToolActivity>,
    /// `Some(true)` for a turn that ended with a result, `Some(false)` for a
    /// turn that ended with an error.
    pub turn_end: Option<bool>,
}

impl Changes {
    fn touch(&mut self, i: usize) {
        if !self.messages.contains(&i) {
            self.messages.push(i);
        }
    }
}

#[derive(Default)]
pub struct Session {
    pub messages: Vec<Message>,
    /// The id of the stored chat, from the first prompt of the chat on.
    pub chat_id: Option<String>,
    session_id: Option<String>,
    /// Working directory of the session. `--resume` only finds the session there.
    session_dir: Option<PathBuf>,
    run: Option<Run>,
    /// PNG files of the images for the next prompt.
    pub attachments: Vec<PathBuf>,
}

impl Session {
    pub fn is_running(&self) -> bool {
        self.run.is_some()
    }

    /// Handles the events of the running prompt.
    pub fn poll(&mut self, cfg: &Config) -> Changes {
        let mut changes = Changes::default();
        let Some(run) = &self.run else {
            return changes;
        };
        while let Some(ev) = run.try_recv() {
            match ev {
                Event::Session(id) => {
                    // Save at once, so that the session survives a quit during the turn.
                    self.session_id = Some(id);
                    self.save(cfg);
                }
                Event::Text(t) => match self.messages.last_mut() {
                    Some(m) if m.role == Role::Assistant => {
                        m.text.push_str(&t);
                        changes.touch(self.messages.len() - 1);
                    }
                    _ if t.trim().is_empty() => {}
                    _ => {
                        self.messages.push(Message::new(Role::Assistant, t));
                        changes.touch(self.messages.len() - 1);
                    }
                },
                Event::ToolUse(name) => {
                    self.messages
                        .push(Message::new(Role::Note, format!("Tool: {name}")));
                    changes.touch(self.messages.len() - 1);
                    changes.tools.push(ToolActivity::Use(name));
                }
                Event::ToolError => changes.tools.push(ToolActivity::Error),
                Event::Finished { error } => {
                    changes.turn_end = Some(match error {
                        Some(e) => {
                            self.messages.push(Message::new(Role::Error, e));
                            changes.touch(self.messages.len() - 1);
                            false
                        }
                        None => true,
                    });
                    break;
                }
            }
        }
        if changes.turn_end.is_some() {
            self.run = None;
            self.save(cfg);
        }
        changes
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
    pub fn prompt_dir(&self, cfg: &Config) -> Option<PathBuf> {
        if self.session_id.is_some() {
            self.session_dir.clone()
        } else {
            cfg.working_dir()
        }
    }

    /// Sends the prompt `text` with the attached images. `notify` runs after
    /// each event of `claude`. Returns false if there is nothing to send or
    /// a prompt runs.
    pub fn send(&mut self, text: &str, cfg: &Config, notify: impl Fn() + Send + 'static) -> bool {
        let prompt = text.trim().to_owned();
        if (prompt.is_empty() && self.attachments.is_empty()) || self.run.is_some() {
            return false;
        }
        let images = std::mem::take(&mut self.attachments);
        let dir = self.prompt_dir(cfg);
        self.session_dir = dir.clone();
        if self.chat_id.is_none() {
            self.chat_id = Some(SavedChat::new_id());
        }
        let mut message = Message::new(Role::User, prompt.clone());
        message.images = images.clone();
        self.messages.push(message);
        let req = Request {
            program: cfg.claude_path.trim().to_owned(),
            working_dir: dir,
            model: cfg.model_arg(),
            extra_args: [cfg.permission_args(), cfg.instruction_args()].concat(),
            resume: self.session_id.clone(),
            prompt,
            images,
        };
        match Run::start(req, notify) {
            Ok(run) => self.run = Some(run),
            Err(e) => self.messages.push(Message::new(Role::Error, e)),
        }
        true
    }

    /// Stops the running prompt, if any.
    pub fn stop(&mut self) {
        if let Some(run) = &self.run {
            run.stop();
        }
    }

    /// Starts an empty chat.
    pub fn new_chat(&mut self) {
        self.run = None;
        self.messages.clear();
        self.chat_id = None;
        self.session_id = None;
        self.session_dir = None;
    }

    /// Opens a stored chat. The next prompt continues its conversation.
    pub fn open(&mut self, chat: SavedChat) {
        self.run = None;
        self.messages = chat.messages;
        self.chat_id = Some(chat.id);
        self.session_id = Some(chat.session_id);
        self.session_dir = chat.working_dir;
    }

    /// Deletes the stored chat `id`. The open chat then gets a new id, so
    /// that its next prompt stores it again.
    pub fn delete(&mut self, id: &str) {
        SavedChat::delete(id);
        if self.chat_id.as_deref() == Some(id) {
            self.chat_id = Some(SavedChat::new_id());
        }
    }

    /// Adds the image `path` to the next prompt, once.
    pub fn attach(&mut self, path: PathBuf) {
        if !self.attachments.contains(&path) {
            self.attachments.push(path);
        }
    }
}
