//! Shell-free command specifications used by the coding harness.
//!
//! Commands are kept as discrete argv elements until they reach
//! `soul_sandbox`. There is no shell parser or string re-splitting in this
//! layer. `CheckSpec` remains a compact human-readable command string for
//! persistence, so its parser deliberately accepts only whitespace-separated
//! argv and rejects shell control syntax.

use soul_sandbox::SpawnSpec;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandArg {
    /// An argument selected by harness code, such as a Git subcommand or a
    /// command-line flag.
    Flag(String),
    /// A value originating from a task, path, or other external input.
    Value(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    program: String,
    args: Vec<CommandArg>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>) -> Result<Self, CommandSpecError> {
        let program = program.into();
        if program.trim().is_empty() {
            return Err(CommandSpecError::EmptyProgram);
        }
        if program.contains('\0') || program.chars().any(char::is_whitespace) {
            return Err(CommandSpecError::InvalidProgram(program));
        }

        Ok(Self {
            program,
            args: Vec::new(),
        })
    }

    pub fn flag(mut self, argument: impl Into<String>) -> Self {
        self.args.push(CommandArg::Flag(argument.into()));
        self
    }

    pub fn value(mut self, argument: impl Into<String>) -> Self {
        self.args.push(CommandArg::Value(argument.into()));
        self
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[CommandArg] {
        &self.args
    }

    pub fn to_spawn_spec(&self) -> SpawnSpec {
        let mut spec = SpawnSpec::new(self.program.clone());
        for argument in &self.args {
            spec = match argument {
                CommandArg::Flag(argument) => spec.flag(argument.clone()),
                CommandArg::Value(argument) => spec.value(argument.clone()),
            };
        }
        spec
    }

    /// Parse the persisted `CheckSpec.command` form.
    ///
    /// This is intentionally not a shell parser. Quoting, command
    /// substitution, redirection, pipes, and separators are rejected instead
    /// of being interpreted ambiguously. Checks that need a value containing
    /// whitespace should be represented by a future structured task contract.
    pub fn parse(command: &str) -> Result<Self, CommandSpecError> {
        if command.trim().is_empty() {
            return Err(CommandSpecError::EmptyCommand);
        }
        if command.contains('\0') {
            return Err(CommandSpecError::NulByte);
        }
        if command
            .chars()
            .any(|character| matches!(character, '\n' | '\r' | ';' | '|' | '&' | '>' | '<' | '`'))
            || command.contains("$(")
        {
            return Err(CommandSpecError::ShellSyntax);
        }

        let mut parts = command.split_whitespace();
        let program = parts.next().ok_or(CommandSpecError::EmptyCommand)?;
        let mut spec = Self::new(program.to_string())?;
        for argument in parts {
            // Parsed checks are authored as a contract, not interpolated into
            // a shell. Keeping each token as a literal program argument means
            // `--flag` is still a valid compiler/test flag without making a
            // shell injection possible.
            spec = spec.flag(argument.to_string());
        }
        Ok(spec)
    }

    pub fn display(&self) -> String {
        let mut parts = vec![self.program.clone()];
        parts.extend(self.args.iter().map(|argument| match argument {
            CommandArg::Flag(argument) | CommandArg::Value(argument) => argument.clone(),
        }));
        parts.join(" ")
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CommandSpecError {
    #[error("command cannot be empty")]
    EmptyCommand,
    #[error("command program cannot be empty")]
    EmptyProgram,
    #[error("command program is invalid: {0}")]
    InvalidProgram(String),
    #[error("command contains a NUL byte")]
    NulByte,
    #[error("command contains shell syntax; checks must be shell-free argv")]
    ShellSyntax,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsed_checks_are_shell_free_and_preserve_flags() {
        let command = CommandSpec::parse("cargo test -p soul-coding").unwrap();
        assert_eq!(command.program(), "cargo");
        assert_eq!(command.display(), "cargo test -p soul-coding");
        assert!(matches!(
            CommandSpec::parse("cargo test | sh"),
            Err(CommandSpecError::ShellSyntax)
        ));
    }

    #[test]
    fn structured_values_keep_spaces_until_exec() {
        let command = CommandSpec::new("echo").unwrap().value("two words");
        assert_eq!(
            command.to_spawn_spec().preview().unwrap(),
            vec!["echo".to_string(), "two words".to_string()]
        );
    }
}
