use libloading::{Library, Symbol};
use anyhow::{Context, Result, anyhow};
use std::sync::Arc;
use ffi_bridge::SkillExecuteFn;

pub struct SkillInstance {
    lib: Arc<Library>,
    pub execute_fn: Symbol<'static, SkillExecuteFn>,
}

impl SkillInstance {
    pub fn load(path: &std::path::Path) -> Result<Self> {
        unsafe {
            let lib = Arc::new(Library::new(path)
                .context("Failed to load dynamic library")?);

            let execute_fn: Symbol<SkillExecuteFn> = lib.get(b"skill_execute\0")
                .context("Symbol 'skill_execute' not found in library")?;

            let execute_fn_static: Symbol<'static, SkillExecuteFn> =
                std::mem::transmute(execute_fn);

            Ok(Self {
                lib,
                execute_fn: execute_fn_static,
            })
        }
    }

    pub fn run(&self, input: &[u8], output: &mut [u8]) -> Result<()> {
        if output.len() < input.len() {
            return Err(anyhow!("Output buffer too small"));
        }

        unsafe {
            (self.execute_fn)(
                input.as_ptr(),
                output.as_mut_ptr(),
                input.len()
            );
        }
        Ok(())
    }
}

pub struct SkillManager {
    active_skill: Option<SkillInstance>,
}

impl SkillManager {
    pub fn new() -> Self {
        Self { active_skill: None }
    }

    pub fn swap_skill(&mut self, path: &std::path::Path) -> Result<()> {
        let new_skill = SkillInstance::load(path)?;
        self.active_skill = Some(new_skill);
        Ok(())
    }

    pub fn execute(&self, input: &[u8], output: &mut [u8]) -> Result<()> {
        match &self.active_skill {
            Some(skill) => skill.run(input, output),
            None => Err(anyhow!("No skill currently loaded")),
        }
    }
}
