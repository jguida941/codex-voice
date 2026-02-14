//! Clickable-button registry that keeps HUD hitboxes in sync with rendered rows.
//!
//! Tracks button positions on the HUD and maps mouse clicks to actions.

use crate::input::InputEvent;
use std::sync::{Arc, RwLock};

/// A clickable button region on the HUD.
#[derive(Debug, Clone)]
pub struct Button {
    /// Start x position (1-based, inclusive)
    pub start_x: u16,
    /// End x position (1-based, inclusive)
    pub end_x: u16,
    /// Y position (row, 1-based)
    pub y: u16,
    /// Event to emit when clicked
    pub event: ButtonAction,
}

/// Button action to perform on click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    VoiceTrigger,
    ToggleAutoVoice,
    ToggleSendMode,
    SettingsToggle,
    ToggleHudStyle,
    /// Return to full HUD from minimal mode.
    HudBack,
    HelpToggle,
    ThemePicker,
}

impl ButtonAction {
    /// Convert to InputEvent for dispatch.
    #[allow(dead_code)]
    pub fn to_input_event(self) -> InputEvent {
        match self {
            ButtonAction::VoiceTrigger => InputEvent::VoiceTrigger,
            ButtonAction::ToggleAutoVoice => InputEvent::ToggleAutoVoice,
            ButtonAction::ToggleSendMode => InputEvent::ToggleSendMode,
            ButtonAction::SettingsToggle => InputEvent::SettingsToggle,
            ButtonAction::ToggleHudStyle => InputEvent::ToggleHudStyle,
            ButtonAction::HudBack => InputEvent::ToggleHudStyle,
            ButtonAction::HelpToggle => InputEvent::HelpToggle,
            ButtonAction::ThemePicker => InputEvent::ThemePicker,
        }
    }
}

/// Thread-safe registry of clickable buttons.
#[derive(Debug, Default, Clone)]
pub struct ButtonRegistry {
    buttons: Arc<RwLock<Vec<Button>>>,
    /// Y offset from bottom of terminal where HUD starts
    hud_bottom_offset: Arc<RwLock<u16>>,
}

impl ButtonRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all registered buttons.
    pub fn clear(&self) {
        if let Ok(mut buttons) = self.buttons.write() {
            buttons.clear();
        }
    }

    /// Register a button at the given position.
    pub fn register(&self, start_x: u16, end_x: u16, y: u16, event: ButtonAction) {
        if let Ok(mut buttons) = self.buttons.write() {
            buttons.push(Button {
                start_x,
                end_x,
                y,
                event,
            });
        }
    }

    /// Set the HUD's vertical offset from terminal bottom.
    pub fn set_hud_offset(&self, offset: u16) {
        if let Ok(mut o) = self.hud_bottom_offset.write() {
            *o = offset;
        }
    }

    /// Get the HUD's vertical offset from terminal bottom.
    pub fn hud_offset(&self) -> u16 {
        self.hud_bottom_offset.read().map(|o| *o).unwrap_or(0)
    }

    /// Find a button at the given terminal coordinates.
    /// Returns the action if a button was clicked.
    pub fn find_at(&self, x: u16, y: u16, terminal_height: u16) -> Option<ButtonAction> {
        let buttons = self.buttons.read().ok()?;
        let hud_offset = self.hud_offset();

        // Convert terminal y to HUD-relative y
        // HUD rows are counted from bottom: row 1 is bottom border, row 4 is top
        // Terminal reports y from top (1-based)
        let hud_y = terminal_height.saturating_sub(y) + 1;

        // Only check if click is within HUD area
        if hud_y > hud_offset || hud_y == 0 {
            return None;
        }

        for button in buttons.iter() {
            if button.y == hud_y && x >= button.start_x && x <= button.end_x {
                return Some(button.event);
            }
        }
        None
    }

    /// Get all registered buttons (for debugging).
    #[allow(dead_code)]
    pub fn all_buttons(&self) -> Vec<Button> {
        self.buttons.read().map(|b| b.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_registry_registers_and_finds() {
        let registry = ButtonRegistry::new();
        registry.set_hud_offset(4);
        registry.register(2, 6, 1, ButtonAction::VoiceTrigger);
        registry.register(8, 14, 1, ButtonAction::ToggleAutoVoice);

        // Terminal height 24, click at y=24 (bottom row) = HUD row 1
        assert_eq!(
            registry.find_at(3, 24, 24),
            Some(ButtonAction::VoiceTrigger)
        );
        assert_eq!(
            registry.find_at(10, 24, 24),
            Some(ButtonAction::ToggleAutoVoice)
        );
        // Click outside buttons
        assert_eq!(registry.find_at(7, 24, 24), None);
        // Click above HUD
        assert_eq!(registry.find_at(3, 10, 24), None);
    }

    #[test]
    fn button_action_converts_to_input_event() {
        assert_eq!(
            ButtonAction::VoiceTrigger.to_input_event(),
            InputEvent::VoiceTrigger
        );
        assert_eq!(
            ButtonAction::HelpToggle.to_input_event(),
            InputEvent::HelpToggle
        );
    }
}
