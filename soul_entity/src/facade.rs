use parking_lot::Mutex;
use soul_agent_contracts::{AgentContext, AgentEvent, HookHub, LogHook, SkillRegistry};
use std::collections::HashMap;
use std::sync::Arc;

// ── Facade d'agent : hooks + skills (intégré à l'entité) ────

pub struct AgentFacade {
    pub hooks: HookHub,
    pub skills: SkillRegistry,
    pub contexts: Mutex<HashMap<String, AgentContext>>,
}

impl Default for AgentFacade {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentFacade {
    pub fn new() -> Self {
        let hooks = HookHub::new();
        hooks.register(Arc::new(LogHook));
        Self {
            hooks,
            skills: SkillRegistry::new(),
            contexts: Mutex::new(HashMap::new()),
        }
    }

    pub fn fire(&self, event: &AgentEvent) {
        let ctx = AgentContext::new("soul entity");
        self.hooks.fire(event, &ctx);
    }

    pub fn hook_count(&self) -> usize {
        self.hooks.count()
    }

    pub fn skill_count(&self) -> usize {
        self.skills.count()
    }
}
