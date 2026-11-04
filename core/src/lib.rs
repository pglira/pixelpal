//! The parts of pixelpal that the pal process and the chat process share: the
//! configuration, the stored chats and images, the `claude` runner, the
//! progress of Link, and the messages between the two processes.

pub mod claude;
pub mod config;
pub mod files;
pub mod history;
pub mod images;
pub mod ipc;
pub mod picker;
pub mod progress;
pub mod theme;
