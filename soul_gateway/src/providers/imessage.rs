//! Outbound iMessage adapter for the macOS Messages app.
//!
//! Apple does not expose a general headless iMessage bot API. This adapter is
//! deliberately macOS-only and requires explicit Apple Events permission.

use super::{ChannelConfig, ChannelProvider};

pub struct IMessageProvider {
    enabled: bool,
}

impl IMessageProvider {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("SOULSYSTEM_IMESSAGE_ENABLED")
                .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
                .unwrap_or(false),
        }
    }

    pub fn enabled() -> Self {
        Self { enabled: true }
    }
}

#[async_trait::async_trait]
impl ChannelProvider for IMessageProvider {
    fn name(&self) -> &'static str {
        "imessage"
    }

    async fn start(
        &self,
        _cfg: ChannelConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.enabled {
            return Err("SOULSYSTEM_IMESSAGE_ENABLED is not true".into());
        }
        if !cfg!(target_os = "macos") {
            return Err("iMessage is only available on macOS".into());
        }
        Ok(())
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.enabled {
            return Err("SOULSYSTEM_IMESSAGE_ENABLED is not true".into());
        }
        if !cfg!(target_os = "macos") {
            return Err("iMessage is only available on macOS".into());
        }

        let script = r#"on run argv
set recipient to item 1 of argv
set messageText to item 2 of argv
tell application "Messages"
  set targetService to 1st service whose service type = iMessage
  set targetBuddy to buddy recipient of targetService
  send messageText to targetBuddy
end tell
end run"#;
        let status = tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .arg("--")
            .arg(chat_id)
            .arg(text)
            .status()
            .await?;
        if !status.success() {
            return Err(format!("Messages automation failed with {status}").into());
        }
        Ok(())
    }
}
