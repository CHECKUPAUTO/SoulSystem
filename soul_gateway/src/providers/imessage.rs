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

/// Whether `chat_id` is a plausible iMessage recipient.
///
/// The AppleScript below takes its values from `argv`, so `chat_id` can never
/// become script *source* — there is no quoting boundary to cross. What it can
/// do is name a recipient, and an unconstrained value there means a caller
/// chooses who receives the message.
///
/// Accepts an e-mail address or a phone number (digits, optional leading `+`,
/// with spaces, dashes and parentheses tolerated). Anything else is refused
/// rather than normalised: guessing what a malformed recipient "meant" is how
/// a message reaches the wrong person.
fn is_plausible_recipient(chat_id: &str) -> bool {
    if chat_id.is_empty() || chat_id.len() > 254 {
        return false;
    }
    // Leading or trailing whitespace means the caller passed something it
    // never parsed. Internal spacing is fine — "+1 (555) 123-4567" is how
    // people write phone numbers — but trimming here would be normalising,
    // which this function deliberately does not do.
    if chat_id.trim() != chat_id {
        return false;
    }
    let looks_like_email = {
        let mut parts = chat_id.split('@');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(user), Some(host), None) => {
                !user.is_empty()
                    && host.contains('.')
                    && !host.starts_with('.')
                    && !host.ends_with('.')
                    && !chat_id.chars().any(char::is_whitespace)
            }
            _ => false,
        }
    };
    let looks_like_phone = {
        let digits = chat_id.chars().filter(|c| c.is_ascii_digit()).count();
        digits >= 5
            && chat_id
                .chars()
                .all(|c| c.is_ascii_digit() || " +-()".contains(c))
    };
    looks_like_email || looks_like_phone
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
        if !is_plausible_recipient(chat_id) {
            return Err(format!("refusing to send: implausible recipient {chat_id:?}").into());
        }

        // Fixed argv, and the script is a literal that reads its values from
        // `argv` — `chat_id` and `text` are bound to AppleScript variables,
        // never spliced into the script source. This is the parameterised
        // form; interpolating them into the `-e` string would be the
        // injectable one.
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

#[cfg(test)]
mod recipient_tests {
    use super::is_plausible_recipient;

    #[test]
    fn plausible_recipients_are_accepted() {
        for ok in [
            "someone@example.com",
            "+1 (555) 123-4567",
            "5551234567",
            "+33612345678",
        ] {
            assert!(is_plausible_recipient(ok), "should accept: {ok:?}");
        }
    }

    #[test]
    fn implausible_recipients_are_refused() {
        for bad in [
            "",
            "not a recipient",
            "@example.com",
            "user@",
            "user@nodot",
            "123",
            "a@b.c d",
        ] {
            assert!(!is_plausible_recipient(bad), "should reject: {bad:?}");
        }
    }

    /// The recipient is refused, not rewritten into something acceptable.
    #[test]
    fn a_malformed_recipient_is_not_normalised() {
        assert!(!is_plausible_recipient("  +1 555 123 4567  "));
    }
}
