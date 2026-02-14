//! Settings-menu state and navigation rules so keyboard interactions stay predictable.

use super::items::{SettingsItem, SETTINGS_ITEMS};

#[derive(Debug, Clone)]
pub struct SettingsMenuState {
    pub selected: usize,
}

impl SettingsMenuState {
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    pub fn selected_item(&self) -> SettingsItem {
        SETTINGS_ITEMS
            .get(self.selected)
            .copied()
            .unwrap_or(SettingsItem::AutoVoice)
    }

    pub fn move_up(&mut self) {
        if self.selected == 0 {
            self.selected = SETTINGS_ITEMS.len().saturating_sub(1);
        } else {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    pub fn move_down(&mut self) {
        if SETTINGS_ITEMS.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % SETTINGS_ITEMS.len();
    }
}
