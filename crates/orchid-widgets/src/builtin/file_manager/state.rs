//! Tab / pane state for the file manager.

use uuid::Uuid;

use super::config::{SortBy, ViewMode};
use super::selection::SelectionModel;

/// One tab inside a pane.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct TabState {
    pub id: Uuid,
    pub path: orchid_fs::FsPath,
    pub history_back: Vec<orchid_fs::FsPath>,
    pub history_forward: Vec<orchid_fs::FsPath>,
    pub view_mode: ViewMode,
    pub selection: SelectionModel,
    pub quick_filter: String,
    pub scroll_position: f32,
    pub sort_by: SortBy,
    pub sort_descending: bool,
}

impl TabState {
    /// Build a tab pointing at `path`.
    #[must_use]
    pub fn new(path: orchid_fs::FsPath, view_mode: ViewMode, sort_by: SortBy) -> Self {
        Self {
            id: Uuid::new_v4(),
            path,
            history_back: Vec::new(),
            history_forward: Vec::new(),
            view_mode,
            selection: SelectionModel::new(),
            quick_filter: String::new(),
            scroll_position: 0.0,
            sort_by,
            sort_descending: false,
        }
    }

    /// Navigate to `new_path`, pushing the previous path onto the back
    /// history and clearing the forward history.
    pub fn navigate_to(&mut self, new_path: orchid_fs::FsPath) {
        if self.path == new_path {
            return;
        }
        self.history_back.push(self.path.clone());
        self.history_forward.clear();
        self.path = new_path;
        self.selection.clear();
        self.quick_filter.clear();
        self.scroll_position = 0.0;
    }

    /// Pop from back history; pushes the current path on the forward
    /// stack. Returns `true` if the path actually changed.
    pub fn back(&mut self) -> bool {
        let Some(prev) = self.history_back.pop() else {
            return false;
        };
        self.history_forward.push(self.path.clone());
        self.path = prev;
        self.selection.clear();
        true
    }

    /// Inverse of [`Self::back`].
    pub fn forward(&mut self) -> bool {
        let Some(next) = self.history_forward.pop() else {
            return false;
        };
        self.history_back.push(self.path.clone());
        self.path = next;
        self.selection.clear();
        true
    }
}

/// One pane (left or right) with its tabs.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct PaneState {
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
}

impl PaneState {
    /// Build a pane with a single tab.
    #[must_use]
    pub fn with_single_tab(tab: TabState) -> Self {
        Self {
            tabs: vec![tab],
            active_tab: 0,
        }
    }

    /// Reference to the active tab.
    #[must_use]
    pub fn active_tab(&self) -> &TabState {
        &self.tabs[self.active_tab.min(self.tabs.len().saturating_sub(1))]
    }

    /// Mutable reference to the active tab.
    pub fn active_tab_mut(&mut self) -> &mut TabState {
        let idx = self.active_tab.min(self.tabs.len().saturating_sub(1));
        &mut self.tabs[idx]
    }
}

/// Which pane is currently focused.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Left,
    Right,
}

/// Full file-manager state.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct FileManagerState {
    pub left_pane: PaneState,
    pub right_pane: Option<PaneState>,
    pub active_pane: ActivePane,
}

impl FileManagerState {
    /// Build a single-pane state rooted at `path`.
    #[must_use]
    pub fn single_pane(path: orchid_fs::FsPath, view_mode: ViewMode, sort_by: SortBy) -> Self {
        Self {
            left_pane: PaneState::with_single_tab(TabState::new(path, view_mode, sort_by)),
            right_pane: None,
            active_pane: ActivePane::Left,
        }
    }

    /// Reference to the active pane.
    #[must_use]
    pub fn active_pane(&self) -> &PaneState {
        match self.active_pane {
            ActivePane::Left => &self.left_pane,
            ActivePane::Right => self
                .right_pane
                .as_ref()
                .unwrap_or(&self.left_pane),
        }
    }

    /// Mutable reference to the active pane.
    pub fn active_pane_mut(&mut self) -> &mut PaneState {
        match self.active_pane {
            ActivePane::Left => &mut self.left_pane,
            ActivePane::Right => match self.right_pane.as_mut() {
                Some(r) => r,
                None => &mut self.left_pane,
            },
        }
    }

    /// Reference to the active tab inside the active pane.
    #[must_use]
    pub fn active_tab(&self) -> &TabState {
        self.active_pane().active_tab()
    }

    /// Mutable reference to the active tab inside the active pane.
    pub fn active_tab_mut(&mut self) -> &mut TabState {
        self.active_pane_mut().active_tab_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> orchid_fs::FsPath {
        orchid_fs::FsPath::new(s).unwrap()
    }

    #[test]
    fn navigate_to_pushes_history() {
        let mut tab = TabState::new(p("local:/a"), ViewMode::Details, SortBy::Name);
        tab.navigate_to(p("local:/b"));
        assert_eq!(tab.history_back.len(), 1);
        assert_eq!(tab.path, p("local:/b"));
        assert!(tab.back());
        assert_eq!(tab.path, p("local:/a"));
        assert_eq!(tab.history_forward.len(), 1);
        assert!(tab.forward());
        assert_eq!(tab.path, p("local:/b"));
    }

    #[test]
    fn back_on_empty_returns_false() {
        let mut tab = TabState::new(p("local:/a"), ViewMode::Details, SortBy::Name);
        assert!(!tab.back());
        assert!(!tab.forward());
    }
}
