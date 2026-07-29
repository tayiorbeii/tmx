//! tmx — lightweight tmux-native workflow layer.
//!
//! tmux is the source of truth for live sessions/windows/panes.
//! SQLite is the durable store for notes, MRU, and projects.
//! No daemon, no server, no web UI. Terminal-native only.

pub mod cli;
pub mod commands;
pub mod config;
pub mod mobile;
pub mod model;
pub mod palette;
pub mod project;
pub mod selector;
pub mod state;
pub mod switcher;
pub mod tmux;
