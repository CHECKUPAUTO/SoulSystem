use parking_lot::Mutex;
use soul_openclaw::{AgentContext, AgentEvent, HookHub, LogHook, SkillRegistry};
use std::collections::HashMap;
use std::sync::Arc;

// ── Facade openclaw : hooks + skills (intégré à l'entité) ────

pub struct OpenClawFacade {
    pub hooks: HookHub,
    pub skills: SkillRegistry,
    pub contexts: Mutex<HashMap<String, AgentContext>>,
}

impl Default for OpenClawFacade {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenClawFacade {
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
