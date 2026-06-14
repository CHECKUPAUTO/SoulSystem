//! The single Level-2 confirmation gate.
//!
//! Honesty invariant #4: *Level 2 (impactful / destructive) actions ALWAYS
//! require confirmation.* This module is the one enforcement point the
//! autonomous loop must route every action through. A `Destructive` action
//! cannot be executed without presenting a matching [`Confirmation`] token —
//! there is no code path that bypasses it.
//!
//! Levels map onto [`soul_tools::PermissionLevel`]:
//!
//! | spec level | `PermissionLevel` | gate decision                |
//! |-----------:|-------------------|------------------------------|
//! | 0 (read)   | `Read`            | `Allow`                      |
//! | 1 (write)  | `Write`           | `AllowWithLog`               |
//! | 2 (impact) | `Destructive`     | `RequireConfirmation`        |

use serde::{Deserialize, Serialize};
use soul_tools::PermissionLevel;

/// What the gate decides for a given action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    /// Level 0 — read-only, run freely.
    Allow,
    /// Level 1 — mutates state; permitted but must be audit-logged.
    AllowWithLog,
    /// Level 2 — impactful; blocked until an explicit confirmation is given.
    RequireConfirmation,
}

impl Decision {
    /// The gate's policy: derive a decision from a permission level.
    pub fn for_level(level: PermissionLevel) -> Decision {
        match level {
            PermissionLevel::Read => Decision::Allow,
            PermissionLevel::Write => Decision::AllowWithLog,
            PermissionLevel::Destructive => Decision::RequireConfirmation,
        }
    }
}

/// An explicit approval for one specific Level-2 action. Produced only by an
/// out-of-band confirmation step (a human/owner saying "yes to *this*").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confirmation {
    /// The exact action string the approval was granted for.
    pub action: String,
    /// Who/what granted it (audit trail).
    pub approved_by: String,
}

impl Confirmation {
    pub fn new(action: impl Into<String>, approved_by: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            approved_by: approved_by.into(),
        }
    }
}

/// Why an action was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GateError {
    /// A Level-2 action was attempted with no confirmation at all.
    #[error("level-2 action requires confirmation: {0}")]
    ConfirmationRequired(String),
    /// A confirmation was supplied but for a different action.
    #[error("confirmation does not match action: approved `{approved}`, attempted `{attempted}`")]
    ConfirmationMismatch { approved: String, attempted: String },
}

/// The central permission gate. Cheap to construct; hold one per agent loop.
#[derive(Debug, Default, Clone)]
pub struct PermissionGate;

impl PermissionGate {
    pub fn new() -> Self {
        Self
    }

    /// Classify a shell command and decide how it may proceed.
    pub fn assess_command(&self, command: &str) -> Decision {
        Decision::for_level(PermissionLevel::from_command(command))
    }

    /// Authorize an action, enforcing invariant #4.
    ///
    /// - Level 0/1 → `Ok(Decision)`, run (logging for Level 1).
    /// - Level 2 → `Ok(AllowWithLog)` **only** if a matching confirmation is
    ///   supplied; otherwise `Err`. There is deliberately no way to authorize
    ///   a destructive action without a confirmation whose `action` matches.
    pub fn authorize(
        &self,
        command: &str,
        confirmation: Option<&Confirmation>,
    ) -> Result<Decision, GateError> {
        match self.assess_command(command) {
            Decision::Allow => Ok(Decision::Allow),
            Decision::AllowWithLog => Ok(Decision::AllowWithLog),
            Decision::RequireConfirmation => match confirmation {
                None => Err(GateError::ConfirmationRequired(command.to_string())),
                Some(c) if c.action == command => Ok(Decision::AllowWithLog),
                Some(c) => Err(GateError::ConfirmationMismatch {
                    approved: c.action.clone(),
                    attempted: command.to_string(),
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_map_to_decisions() {
        let g = PermissionGate::new();
        assert_eq!(g.assess_command("ls -la"), Decision::Allow);
        assert_eq!(g.assess_command("rm foo.txt"), Decision::AllowWithLog);
        assert_eq!(
            g.assess_command("sudo rm -rf /"),
            Decision::RequireConfirmation
        );
    }

    #[test]
    fn read_and_write_run_without_confirmation() {
        let g = PermissionGate::new();
        assert_eq!(g.authorize("cat README.md", None).unwrap(), Decision::Allow);
        assert_eq!(
            g.authorize("mkdir build", None).unwrap(),
            Decision::AllowWithLog
        );
    }

    #[test]
    fn destructive_without_confirmation_is_blocked() {
        let g = PermissionGate::new();
        let err = g.authorize("mkfs /dev/sda", None).unwrap_err();
        assert!(matches!(err, GateError::ConfirmationRequired(_)));
    }

    #[test]
    fn destructive_with_matching_confirmation_proceeds() {
        let g = PermissionGate::new();
        let cmd = "dd if=/dev/zero of=/dev/sdb";
        let conf = Confirmation::new(cmd, "owner");
        assert_eq!(
            g.authorize(cmd, Some(&conf)).unwrap(),
            Decision::AllowWithLog
        );
    }

    #[test]
    fn confirmation_for_another_action_is_rejected() {
        let g = PermissionGate::new();
        let conf = Confirmation::new("rm -rf /home/old", "owner");
        let err = g.authorize("mkfs /dev/sda", Some(&conf)).unwrap_err();
        assert!(matches!(err, GateError::ConfirmationMismatch { .. }));
    }
}
