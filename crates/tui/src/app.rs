//! Browser state: the bills (newest first), the filter flag and the selection.

use crossterm::event::KeyCode;
use hauz_core::bill::{Bill, Status};

/// What the event loop does after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep running.
    Continue,
    /// Leave the browser.
    Quit,
}

/// The browser's state; holds no I/O.
#[derive(Debug, Clone)]
pub struct App {
    /// Newest-ingested first.
    bills: Vec<Bill>,
    needs_review_only: bool,
    /// Index into the visible list.
    selected: usize,
}

impl App {
    /// Builds the browser from bills in `list` order (oldest first); shows newest first.
    #[must_use]
    pub fn new(mut bills: Vec<Bill>) -> Self {
        bills.reverse();
        Self {
            bills,
            needs_review_only: false,
            selected: 0,
        }
    }

    /// Applies one key press.
    #[allow(clippy::wildcard_enum_match_arm)] // every other key is deliberately ignored
    pub fn on_key(&mut self, key: KeyCode) -> Flow {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => return Flow::Quit,
            KeyCode::Down | KeyCode::Char('j') => {
                let last = self.visible().len().saturating_sub(1);
                self.selected = (self.selected + 1).min(last);
            }
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('r') => {
                self.needs_review_only = !self.needs_review_only;
                self.selected = 0;
            }
            // Every other key is deliberately ignored.
            _ => {}
        }
        Flow::Continue
    }

    /// The selected bill, `None` when the visible list is empty.
    #[must_use]
    pub fn selected(&self) -> Option<&Bill> {
        self.visible().get(self.selected).copied()
    }

    /// Whether the `needs_review` filter is on.
    #[must_use]
    pub fn needs_review_only(&self) -> bool {
        self.needs_review_only
    }

    /// The bills currently shown, newest first.
    pub(crate) fn visible(&self) -> Vec<&Bill> {
        self.bills
            .iter()
            .filter(|b| !self.needs_review_only || b.status() == Status::NeedsReview)
            .collect()
    }

    /// Index of the selection within [`App::visible`].
    pub(crate) fn selected_index(&self) -> usize {
        self.selected
    }
}
