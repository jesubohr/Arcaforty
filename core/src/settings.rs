use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// Preferences. Stored unencrypted because they hold no secrets; values are clamped on load.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    pub idle_lock_minutes: u32,
    pub clipboard_clear_seconds: u32,
    pub reveal_seconds: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            appearance: Appearance::System,
            idle_lock_minutes: 5,
            clipboard_clear_seconds: 30,
            reveal_seconds: 15,
        }
    }
}

impl Settings {
    pub const IDLE_MINUTES: [u32; 5] = [1, 2, 5, 15, 30];
    pub const CLIPBOARD_SECONDS: [u32; 4] = [10, 30, 60, 120];
    pub const REVEAL_SECONDS: [u32; 4] = [5, 15, 30, 60];

    pub fn clamped(mut self) -> Self {
        self.idle_lock_minutes = self.idle_lock_minutes.clamp(1, 120);
        self.clipboard_clear_seconds = self.clipboard_clear_seconds.clamp(5, 600);
        self.reveal_seconds = self.reveal_seconds.clamp(3, 300);
        self
    }
}
