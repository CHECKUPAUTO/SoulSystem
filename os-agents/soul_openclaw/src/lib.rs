//! # soul_openclaw — Bridge vers le framework openclaw
//!
//! openclaw (https://openclaw.ai) est un assistant IA personnel écrit en
//! TypeScript/Node. Cette crate **ne duplique pas** openclaw (qui est
//! écrit en JS) : elle adapte ses concepts — `agent-loop`, `tools`,
//! `memory-host`, `skills`, `hooks` — au runtime Rust de SoulSystem.
//!
//! Le bridge peut fonctionner en deux modes :
//! 1. **Natif** : la boucle d'agent et les outils sont implémentés en
//!    Rust directement (par défaut, autonome, sans Node).
//! 2. **Piloté** : si openclaw tourne quelque part (HTTP/WS), l'entité
//!    peut lui déléguer certaines décisions en POSTant des
//!    `OpenclawRequest` à un endpoint configurable.
//!
//! Cette crate fournit le **contrat de données** et un client HTTP
//! minimal pour le mode piloté. Le mode natif est implémenté dans
//! `soul_kernel` / `soul_orchestrator` et utilise les types définis ici.

// ── Types d'événements alignés sur openclaw agent-loop ───────

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Rôle d'un message dans une conversation, calqué sur openclaw.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Un message échangé avec l'agent (équivalent de `AgentMessage` openclaw).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub role: Role,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
    pub ts: DateTime<Utc>,
}

impl AgentMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: Role::User,
            content: content.into(),
            tool_call_id: None,
            name: None,
            ts: Utc::now(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: Role::Assistant,
            content: content.into(),
            tool_call_id: None,
            name: None,
            ts: Utc::now(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: Role::System,
            content: content.into(),
            tool_call_id: None,
            name: None,
            ts: Utc::now(),
        }
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            name: Some(name.into()),
            ts: Utc::now(),
        }
    }
}

/// Contexte d'agent (mémoire court-terme du tour) — équivalent `AgentContext`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentContext {
    pub messages: Vec<AgentMessage>,
    pub system_prompt: String,
    pub metadata: HashMap<String, String>,
}

impl AgentContext {
    pub fn new(system_prompt: impl Into<String>) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: system_prompt.into(),
            metadata: HashMap::new(),
        }
    }

    pub fn push(&mut self, msg: AgentMessage) {
        self.messages.push(msg);
    }
}

/// Outil invocable par l'agent (équivalent `AgentTool` openclaw).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTool {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
}

impl AgentTool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters_schema: schema,
        }
    }
}

/// Appel d'outil (généré par l'assistant).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Réponse d'un tour d'agent (équivalent `AgentEvent`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    /// L'agent a émis du texte.
    TextDelta { content: String },
    /// L'agent veut appeler un outil.
    ToolCall { call: ToolCall },
    /// L'agent a terminé le tour.
    TurnEnd { reason: String },
    /// Erreur rencontrée.
    Error { message: String },
}

/// Configuration de la boucle d'agent (équivalent `AgentLoopConfig`).
#[derive(Debug, Clone)]
pub struct AgentLoopConfig {
    pub max_steps: usize,
    pub max_tokens_per_step: usize,
    pub require_confirmation_for_destructive: bool,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_steps: 8,
            max_tokens_per_step: 2048,
            require_confirmation_for_destructive: true,
        }
    }
}

/// Hook appelé à chaque transition d'étape (concept openclaw).
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    fn on_event(&self, event: &AgentEvent, ctx: &AgentContext);
}

/// Hub de hooks avec un lock interne (concurrent-safe).
pub struct HookHub {
    hooks: Mutex<Vec<Arc<dyn Hook>>>,
}

impl HookHub {
    pub fn new() -> Self {
        Self {
            hooks: Mutex::new(Vec::new()),
        }
    }

    pub fn register(&self, hook: Arc<dyn Hook>) {
        self.hooks.lock().push(hook);
    }

    pub fn fire(&self, event: &AgentEvent, ctx: &AgentContext) {
        for h in self.hooks.lock().iter() {
            h.on_event(event, ctx);
        }
    }

    pub fn count(&self) -> usize {
        self.hooks.lock().len()
    }
}

impl Default for HookHub {
    fn default() -> Self {
        Self::new()
    }
}

/// Hook de log simple.
pub struct LogHook;

impl Hook for LogHook {
    fn name(&self) -> &str {
        "log"
    }
    fn on_event(&self, event: &AgentEvent, _ctx: &AgentContext) {
        match event {
            AgentEvent::TextDelta { content } => {
                tracing::info!("[openclaw] text: {}", content);
            }
            AgentEvent::ToolCall { call } => {
                tracing::info!("[openclaw] tool: {} args={}", call.name, call.arguments);
            }
            AgentEvent::TurnEnd { reason } => {
                tracing::info!("[openclaw] turn end: {}", reason);
            }
            AgentEvent::Error { message } => {
                tracing::warn!("[openclaw] error: {}", message);
            }
        }
    }
}

// ── Skill Registry (équivalent SkillRegistry openclaw) ──────

/// Version sémantique d'une skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SkillVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl std::fmt::Display for SkillVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Définition d'une skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub version: SkillVersion,
    pub description: String,
    pub source: String,
    pub enabled: bool,
}

impl Skill {
    pub fn new(
        name: impl Into<String>,
        version: SkillVersion,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version,
            description: description.into(),
            source: "user".into(),
            enabled: true,
        }
    }
}

pub struct SkillRegistry {
    skills: Mutex<HashMap<String, Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: Mutex::new(HashMap::new()),
        }
    }

    pub fn install(&self, skill: Skill) -> bool {
        let mut s = self.skills.lock();
        if let Some(existing) = s.get(&skill.name) {
            // Refuse downgrade
            if skill.version.major < existing.version.major {
                return false;
            }
        }
        s.insert(skill.name.clone(), skill);
        true
    }

    pub fn get(&self, name: &str) -> Option<Skill> {
        self.skills.lock().get(name).cloned()
    }

    pub fn enable(&self, name: &str, enabled: bool) -> bool {
        let mut s = self.skills.lock();
        if let Some(sk) = s.get_mut(name) {
            sk.enabled = enabled;
            true
        } else {
            false
        }
    }

    pub fn list(&self) -> Vec<Skill> {
        self.skills.lock().values().cloned().collect()
    }

    pub fn count(&self) -> usize {
        self.skills.lock().len()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Client HTTP vers openclaw (mode piloté) ─────────────────

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenclawError {
    #[error("http: {0}")]
    Http(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Configuration du client openclaw.
#[derive(Debug, Clone)]
pub struct OpenclawConfig {
    pub base_url: String,
    pub auth_token: Option<String>,
}

impl Default for OpenclawConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8080".into(),
            auth_token: None,
        }
    }
}

/// Client HTTP minimal vers un openclaw distant. Utilise std lib (pas de
/// reqwest) pour minimiser les deps. Si openclaw n'est pas disponible, le
/// mode natif prend le relais.
pub struct OpenclawClient {
    config: OpenclawConfig,
}

impl OpenclawClient {
    pub fn new(config: OpenclawConfig) -> Self {
        Self { config }
    }

    pub fn is_configured(&self) -> bool {
        !self.config.base_url.is_empty()
    }

    pub fn config(&self) -> &OpenclawConfig {
        &self.config
    }

    /// Envoie un message à openclaw et retourne la réponse brute.
    /// Cette méthode ne fait pas d'I/O réel — elle valide juste la requête.
    /// Les appels réseau effectifs sont réalisés par `soul_gateway` qui
    /// possède un client HTTP complet.
    pub fn build_request(&self, ctx: &AgentContext) -> Result<serde_json::Value, OpenclawError> {
        Ok(serde_json::json!({
            "system": ctx.system_prompt,
            "messages": ctx.messages,
            "metadata": ctx.metadata,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_constructors() {
        let u = AgentMessage::user("hello");
        assert_eq!(u.role, Role::User);
        let a = AgentMessage::assistant("hi");
        assert_eq!(a.role, Role::Assistant);
        let s = AgentMessage::system("you are an agent");
        assert_eq!(s.role, Role::System);
        let t = AgentMessage::tool_result("c1", "42", "calculator");
        assert_eq!(t.role, Role::Tool);
        assert_eq!(t.tool_call_id, Some("c1".into()));
    }

    #[test]
    fn context_push() {
        let mut c = AgentContext::new("be helpful");
        c.push(AgentMessage::user("ping"));
        c.push(AgentMessage::assistant("pong"));
        assert_eq!(c.messages.len(), 2);
    }

    #[test]
    fn skill_registry_install_and_get() {
        let r = SkillRegistry::new();
        let s = Skill::new("calc", SkillVersion::new(1, 0, 0), "calculator");
        assert!(r.install(s));
        assert!(r.get("calc").is_some());
        assert!(r.get("nope").is_none());
    }

    #[test]
    fn skill_registry_refuses_downgrade() {
        let r = SkillRegistry::new();
        r.install(Skill::new("calc", SkillVersion::new(2, 0, 0), "v2"));
        // downgrade major
        let ok = r.install(Skill::new("calc", SkillVersion::new(1, 9, 9), "v1.9"));
        assert!(!ok, "downgrade major interdit");
        // upgrade patch ok
        let ok2 = r.install(Skill::new("calc", SkillVersion::new(2, 0, 1), "v2.0.1"));
        assert!(ok2);
    }

    #[test]
    fn skill_registry_enable_disable() {
        let r = SkillRegistry::new();
        r.install(Skill::new("foo", SkillVersion::new(1, 0, 0), "foo"));
        assert!(r.enable("foo", false));
        assert!(!r.get("foo").unwrap().enabled);
        assert!(!r.enable("unknown", true));
    }

    #[test]
    fn hook_hub_runs_all_hooks() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Counter(AtomicUsize);
        impl Hook for Counter {
            fn name(&self) -> &str {
                "counter"
            }
            fn on_event(&self, _: &AgentEvent, _: &AgentContext) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let hub = HookHub::new();
        hub.register(Arc::new(Counter(AtomicUsize::new(0))));
        hub.register(Arc::new(Counter(AtomicUsize::new(0))));
        let ctx = AgentContext::new("x");
        hub.fire(
            &AgentEvent::TurnEnd {
                reason: "ok".into(),
            },
            &ctx,
        );
        assert_eq!(hub.count(), 2);
    }

    #[test]
    fn openclaw_client_builds_request() {
        let c = OpenclawClient::new(OpenclawConfig::default());
        let ctx = AgentContext::new("be helpful");
        let req = c.build_request(&ctx).unwrap();
        assert_eq!(req["system"], "be helpful");
    }

    #[test]
    fn log_hook_does_not_panic() {
        let h = LogHook;
        let ctx = AgentContext::new("");
        h.on_event(
            &AgentEvent::TextDelta {
                content: "x".into(),
            },
            &ctx,
        );
        h.on_event(
            &AgentEvent::ToolCall {
                call: ToolCall {
                    id: "c1".into(),
                    name: "ls".into(),
                    arguments: serde_json::json!({}),
                },
            },
            &ctx,
        );
        h.on_event(
            &AgentEvent::TurnEnd {
                reason: "end".into(),
            },
            &ctx,
        );
        h.on_event(
            &AgentEvent::Error {
                message: "x".into(),
            },
            &ctx,
        );
    }
}
