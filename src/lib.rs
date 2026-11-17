//! ytmt — общее ядро для двух TUI-плееров:
//! `ytm` (YouTube Music) и `ytt` (обычный YouTube).

pub mod app;
pub mod cli;
pub mod mode;
pub mod mpv;
pub mod ui;
pub mod yt;

#[cfg(test)]
mod tests;
