//! Runs one prompt through the `claude` CLI and streams the reply.
//!
//! The runner starts `claude -p` with `--output-format stream-json`, writes the
//! prompt to stdin, and parses stdout line by line on a reader thread.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value;

pub struct Request {
    pub program: String,
    pub working_dir: Option<PathBuf>,
    /// `None` uses the default model of Claude Code.
    pub model: Option<String>,
    /// Permission and tool flags.
    pub extra_args: Vec<String>,
    pub resume: Option<String>,
    pub prompt: String,
}

#[derive(Debug)]
pub enum Event {
    /// The session id of the conversation.
    Session(String),
    /// A part of the reply text.
    Text(String),
    /// Claude calls a tool.
    ToolUse(String),
    /// The turn ended. `error` holds a message if the turn failed.
    Finished { error: Option<String> },
}

pub struct Run {
    rx: Receiver<Event>,
    child: Arc<Mutex<Option<Child>>>,
    stopped: Arc<Mutex<bool>>,
}

impl Run {
    pub fn start(req: Request, ctx: egui::Context) -> Result<Self, String> {
        let mut cmd = Command::new(&req.program);
        cmd.args([
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
        ]);
        if let Some(model) = &req.model {
            cmd.args(["--model", model]);
        }
        if let Some(id) = &req.resume {
            cmd.args(["--resume", id]);
        }
        cmd.args(&req.extra_args);
        if let Some(dir) = &req.working_dir {
            cmd.current_dir(dir);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Cannot start `{}`: {e}", req.program))?;
        let mut stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let prompt = req.prompt;
        // The prompt goes through stdin, so that variadic flags such as
        // `--allowedTools` cannot consume it.
        thread::spawn(move || {
            let _ = stdin.write_all(prompt.as_bytes());
        });
        let stderr_text = thread::spawn(move || {
            let mut s = String::new();
            let _ = stderr.read_to_string(&mut s);
            s
        });

        let (tx, rx) = channel();
        let child = Arc::new(Mutex::new(Some(child)));
        let stopped = Arc::new(Mutex::new(false));
        let (child2, stopped2) = (child.clone(), stopped.clone());
        thread::spawn(move || {
            let send = |e: Event| {
                let _ = tx.send(e);
                ctx.request_repaint();
            };
            let mut parser = Parser::default();
            // Read raw lines, so that invalid UTF-8 cannot stop the reading.
            // A stopped reader would let `claude` block on a full pipe.
            let mut reader = BufReader::new(stdout);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => parser.line(&String::from_utf8_lossy(&buf), &send),
                }
            }
            // Wait without the lock, so that `stop` can always kill the child.
            let child = child2.lock().unwrap().take();
            let status = child.and_then(|mut c| c.wait().ok());
            let stderr = stderr_text.join().unwrap_or_default();
            let result = parser.result.take();
            let error = if let Some(None) = result {
                None
            } else if *stopped2.lock().unwrap() {
                Some("Stopped.".to_owned())
            } else if let Some(result) = result {
                result
            } else {
                let code = status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown status".into());
                let detail = stderr.trim();
                Some(if detail.is_empty() {
                    format!("`claude` ended without a result ({code}).")
                } else {
                    format!("`claude` ended without a result ({code}):\n{detail}")
                })
            };
            send(Event::Finished { error });
        });
        Ok(Self { rx, child, stopped })
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// Kills the child process. The run then ends with a "Stopped." error.
    pub fn stop(&self) {
        *self.stopped.lock().unwrap() = true;
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Parser state for the JSON lines of one turn.
#[derive(Default)]
struct Parser {
    /// True after the first text delta of the turn.
    got_delta: bool,
    /// True after any text of the turn.
    got_text: bool,
    /// `Some(None)` for a successful result, `Some(Some(msg))` for an error.
    result: Option<Option<String>>,
}

impl Parser {
    fn line(&mut self, line: &str, send: &impl Fn(Event)) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let s = |p: &str| v.pointer(p).and_then(Value::as_str);
        match s("/type") {
            Some("system") if s("/subtype") == Some("init") => {
                if let Some(id) = s("/session_id") {
                    send(Event::Session(id.to_owned()));
                }
            }
            Some("stream_event") => match s("/event/type") {
                Some("content_block_start") => match s("/event/content_block/type") {
                    Some("text") if self.got_text => send(Event::Text("\n\n".into())),
                    Some("tool_use") => {
                        let name = s("/event/content_block/name").unwrap_or("tool");
                        send(Event::ToolUse(name.to_owned()));
                    }
                    _ => {}
                },
                Some("content_block_delta") if s("/event/delta/type") == Some("text_delta") => {
                    if let Some(t) = s("/event/delta/text") {
                        self.got_delta = true;
                        self.got_text = true;
                        send(Event::Text(t.to_owned()));
                    }
                }
                _ => {}
            },
            // Complete messages. They only matter if no partial messages arrive.
            Some("assistant") if !self.got_delta => {
                let blocks = v.pointer("/message/content").and_then(Value::as_array);
                for b in blocks.into_iter().flatten() {
                    if b.get("type").and_then(Value::as_str) == Some("text")
                        && let Some(t) = b.get("text").and_then(Value::as_str)
                    {
                        if self.got_text {
                            send(Event::Text("\n\n".into()));
                        }
                        self.got_text = true;
                        send(Event::Text(t.to_owned()));
                    }
                }
            }
            Some("result") => {
                let is_error = v.get("is_error").and_then(Value::as_bool).unwrap_or(false)
                    || s("/subtype").is_some_and(|st| st != "success");
                let text = s("/result").unwrap_or_default();
                if is_error {
                    let msg = if text.is_empty() {
                        format!("Error: {}", s("/subtype").unwrap_or("unknown"))
                    } else {
                        text.to_owned()
                    };
                    self.result = Some(Some(msg));
                } else {
                    if !self.got_text && !text.is_empty() {
                        self.got_text = true;
                        send(Event::Text(text.to_owned()));
                    }
                    self.result = Some(None);
                }
            }
            _ => {}
        }
    }
}
