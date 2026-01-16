use alacritty_config_derive::ConfigDeserialize;
use serde::Serialize;

/// Math formula rendering configuration.
#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq)]
pub struct MathConfig {
    /// Enable LaTeX math formula rendering.
    pub enabled: bool,

    /// List of program names that enable math rendering.
    /// When non-empty, math rendering is only enabled when the foreground
    /// process matches one of these program names.
    #[serde(default)]
    pub enabled_programs: Vec<String>,
}

impl Default for MathConfig {
    fn default() -> Self {
        Self { enabled: true, enabled_programs: Vec::new() }
    }
}
