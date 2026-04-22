//! Keyboard / paste / mouse → PTY byte encoder.

pub mod keymap;
pub mod paste;

pub use keymap::{InputEncoder, MouseAction, MouseButtonReport, MouseMode};
