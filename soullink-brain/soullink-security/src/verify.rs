//! Secret verification — check if found secrets are live/valid against APIs.

use crate::detectors::Finding;

use std::collections::HashSet;

/// Verification result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerificationResult {
    pub rule_name: String,
    pub verified: bool,
    pub status: String,
}

/// Verify secrets against live APIs (conservative — only checks if key exists, doesn't exploit).
pub struct SecretVerifier {
    client: reqwest::Client,
    checked: HashSet<String>, // avoid double-checking
}

impl Default for SecretVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretVerifier {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("reqwest client");
        Self {
            client,
            checked: HashSet::new(),
        }
    }

    /// Verify a finding against live APIs.
    /// Returns None if not verifiable, Some(result) if checked.
    pub async fn verify(&mut self, finding: &Finding) -> Option<VerificationResult> {
        if !finding.verifiable || self.checked.contains(finding.secret.expose()) {
            return None;
        }

        self.checked.insert(finding.secret.expose().to_owned());

        match finding.rule_name.as_str() {
            "GitHub Personal Access Token" | "GitHub OAuth Access Token" => {
                self.verify_github(finding.secret.expose()).await
            }
            "Slack Bot Token" => self.verify_slack(finding.secret.expose()).await,
            "Stripe Secret Key" => self.verify_stripe(finding.secret.expose()).await,
            "Telegram Bot Token" => self.verify_telegram(finding.secret.expose()).await,
            "Google API Key" => self.verify_google(finding.secret.expose()).await,
            "AWS Access Key ID" => self.verify_aws(finding.secret.expose()).await,
            "GitLab Personal Access Token" => self.verify_gitlab(finding.secret.expose()).await,
            "Binance API Key" => self.verify_binance(finding.secret.expose()).await,
            _ => None,
        }
    }

    async fn verify_github(&self, token: &str) -> Option<VerificationResult> {
        let resp = self
            .client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {token}"))
            .header("User-Agent", "soullink-security/1.0")
            .send()
            .await
            .ok()?;

        let verified = resp.status().is_success();
        Some(VerificationResult {
            rule_name: "GitHub Token".to_string(),
            verified,
            status: if verified {
                "VALID — REV IMMEDIATELY".to_string()
            } else {
                "INVALID".to_string()
            },
        })
    }

    async fn verify_slack(&self, token: &str) -> Option<VerificationResult> {
        let resp = self
            .client
            .post("https://slack.com/api/auth.test")
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .ok()?;

        if let Ok(json) = resp.json::<serde_json::Value>().await {
            let ok = json["ok"].as_bool().unwrap_or(false);
            Some(VerificationResult {
                rule_name: "Slack Token".to_string(),
                verified: ok,
                status: if ok {
                    "VALID — REV IMMEDIATELY".to_string()
                } else {
                    "INVALID".to_string()
                },
            })
        } else {
            None
        }
    }

    async fn verify_stripe(&self, token: &str) -> Option<VerificationResult> {
        // Stripe returns 401 for invalid keys, 200 for valid
        let resp = self
            .client
            .get("https://api.stripe.com/v1/balance")
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .ok()?;

        Some(VerificationResult {
            rule_name: "Stripe Key".to_string(),
            verified: resp.status().is_success(),
            status: if resp.status().is_success() {
                "VALID — REV IMMEDIATELY".to_string()
            } else {
                "INVALID".to_string()
            },
        })
    }

    async fn verify_telegram(&self, token: &str) -> Option<VerificationResult> {
        let resp = self
            .client
            .get(format!("https://api.telegram.org/bot{token}/getMe"))
            .send()
            .await
            .ok()?;

        if let Ok(json) = resp.json::<serde_json::Value>().await {
            let ok = json["ok"].as_bool().unwrap_or(false);
            Some(VerificationResult {
                rule_name: "Telegram Token".to_string(),
                verified: ok,
                status: if ok {
                    "VALID — REV IMMEDIATELY".to_string()
                } else {
                    "INVALID".to_string()
                },
            })
        } else {
            None
        }
    }

    async fn verify_google(&self, _key: &str) -> Option<VerificationResult> {
        // Google API key verification requires specific service endpoints
        None // Too many variants, skip
    }

    async fn verify_aws(&self, _key: &str) -> Option<VerificationResult> {
        // AWS requires SigV4 signing — skip for safety
        None
    }

    async fn verify_gitlab(&self, token: &str) -> Option<VerificationResult> {
        let resp = self
            .client
            .get("https://gitlab.com/api/v4/user")
            .header("PRIVATE-TOKEN", token)
            .send()
            .await
            .ok()?;

        Some(VerificationResult {
            rule_name: "GitLab Token".to_string(),
            verified: resp.status().is_success(),
            status: if resp.status().is_success() {
                "VALID — REV IMMEDIATELY".to_string()
            } else {
                "INVALID".to_string()
            },
        })
    }

    async fn verify_binance(&self, _key: &str) -> Option<VerificationResult> {
        // Binance requires HMAC signing — skip for safety
        None
    }
}
