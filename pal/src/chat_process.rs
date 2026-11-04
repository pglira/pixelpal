//! The chat process: the program `pixelpal-chat`, which shows the chat panel,
//! the menu and the settings window.
//!
//! The pal process starts it and talks to it with the messages of
//! `pixelpal_core::ipc`. A reader thread puts the messages of the chat process
//! into a channel and wakes the event loop.

use std::io::BufReader;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

use pixelpal_core::ipc::{self, ToChat, ToPal};

use crate::wake::Waker;

/// Name of the program of the chat process.
const PROGRAM: &str = "pixelpal-chat";

/// An event of the chat process.
#[derive(Debug)]
pub enum ChatEvent {
    Message(ToPal),
    /// The process with this id ended or closed its output.
    Ended(u32),
}

pub struct ChatProcess {
    child: Option<(Child, ChildStdin)>,
    tx: Sender<ChatEvent>,
    rx: Receiver<ChatEvent>,
    waker: Waker,
    /// The path of the pal program, for the autostart entry.
    pal_exe: PathBuf,
    /// The error of the last start, written only once.
    start_error: Option<String>,
    /// Messages for each new process, before all other messages.
    greeting: Vec<ToChat>,
}

/// The path of the running program. After a reinstall, the path that Linux
/// reports ends with " (deleted)".
pub fn current_exe() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("pixelpal"));
    let s = exe.display().to_string();
    match s.strip_suffix(" (deleted)") {
        Some(p) => PathBuf::from(p),
        None => exe,
    }
}

/// The program of the chat process: `pixelpal-chat` in the directory of the
/// pal program, or else in `PATH`.
fn program() -> PathBuf {
    current_exe()
        .parent()
        .map(|d| d.join(PROGRAM))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(PROGRAM))
}

impl ChatProcess {
    pub fn new(waker: Waker) -> Self {
        let (tx, rx) = channel();
        Self {
            child: None,
            tx,
            rx,
            waker,
            pal_exe: current_exe(),
            start_error: None,
            greeting: Vec::new(),
        }
    }

    /// Sets the messages that each new chat process gets first.
    pub fn set_greeting(&mut self, greeting: Vec<ToChat>) {
        self.greeting = greeting;
    }

    /// The process id of the running chat process.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(|(c, _)| c.id())
    }

    /// The next event, without a wait.
    pub fn try_recv(&mut self) -> Option<ChatEvent> {
        let ev = self.rx.try_recv().ok()?;
        if let ChatEvent::Ended(pid) = ev
            && self.pid() == Some(pid)
        {
            self.reap();
        }
        Some(ev)
    }

    /// Waits for the ended process, so that it does not stay a zombie.
    fn reap(&mut self) {
        if let Some((mut child, stdin)) = self.child.take() {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Starts the chat process if it does not run.
    pub fn start(&mut self) {
        if self.child.is_some() {
            return;
        }
        let program = program();
        let spawned = Command::new(&program)
            .arg("--pal-exe")
            .arg(&self.pal_exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("cannot start {}: {e}", program.display());
                if self.start_error.as_ref() != Some(&msg) {
                    eprintln!("pixelpal: {msg}");
                    self.start_error = Some(msg);
                }
                return;
            }
        };
        self.start_error = None;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let (tx, waker, pid) = (self.tx.clone(), self.waker.clone(), child.id());
        std::thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            while let Some(msg) = ipc::receive::<ToPal>(&mut input) {
                if tx.send(ChatEvent::Message(msg)).is_err() {
                    return;
                }
                waker.wake();
            }
            let _ = tx.send(ChatEvent::Ended(pid));
            waker.wake();
        });
        self.child = Some((child, stdin));
        for msg in self.greeting.clone() {
            self.write(&msg);
        }
    }

    /// Sends `msg` to the chat process, and starts the process first if it
    /// does not run.
    pub fn send(&mut self, msg: &ToChat) {
        self.start();
        self.write(msg);
    }

    /// Sends `msg` to a running chat process.
    fn write(&mut self, msg: &ToChat) {
        if let Some((_, stdin)) = &mut self.child
            && ipc::send(stdin, msg).is_err()
        {
            self.reap();
        }
    }

    /// Sends `msg` only if the chat process runs.
    pub fn send_if_running(&mut self, msg: &ToChat) {
        self.write(msg);
    }
}

impl Drop for ChatProcess {
    /// Closes the input of the chat process, which ends it. A process that
    /// does not end within two seconds is killed.
    fn drop(&mut self) {
        let Some((mut child, stdin)) = self.child.take() else {
            return;
        };
        drop(stdin);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < until {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}
