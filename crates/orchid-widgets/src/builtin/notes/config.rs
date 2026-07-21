//! Persistent config for the notes / scratchpad widget.

#![allow(missing_docs)]

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One scratchpad tab.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct NoteEntry {
    pub id: String,
    pub title: String,
    pub body: String,
}

impl NoteEntry {
    #[must_use]
    pub fn blank() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: String::new(),
            body: String::new(),
        }
    }
}

/// Persisted notes widget state.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct NotesConfig {
    pub notes: Vec<NoteEntry>,
    pub active_index: u32,
    /// Editor font size in points (clamped 10–32).
    pub font_size: u8,
    pub word_wrap: bool,
    pub mono_font: bool,
    pub show_status_bar: bool,
}

impl Default for NotesConfig {
    fn default() -> Self {
        Self {
            notes: vec![NoteEntry::blank()],
            active_index: 0,
            font_size: 14,
            word_wrap: true,
            mono_font: false,
            show_status_bar: true,
        }
    }
}

impl NotesConfig {
    /// Clamp font size into the supported range.
    pub fn clamp_font_size(size: u8) -> u8 {
        size.clamp(10, 32)
    }

    /// Ensure at least one note and a valid active index.
    pub fn normalize(&mut self) {
        if self.notes.is_empty() {
            self.notes.push(NoteEntry::blank());
        }
        self.font_size = Self::clamp_font_size(self.font_size);
        let max = (self.notes.len().saturating_sub(1)) as u32;
        if self.active_index > max {
            self.active_index = max;
        }
    }

    #[must_use]
    pub fn active_note(&self) -> &NoteEntry {
        self.notes
            .get(self.active_index as usize)
            .unwrap_or(&self.notes[0])
    }

    pub fn active_note_mut(&mut self) -> &mut NoteEntry {
        self.normalize();
        let idx = self.active_index as usize;
        &mut self.notes[idx]
    }
}
