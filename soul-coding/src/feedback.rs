//! User feedback events for the future preference-learning layer.
//!
//! These events are intentionally only an auditable record. They do not claim
//! to reproduce or implement Command Code's proprietary taste-1 model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FeedbackKind {
    Accepted,
    Rejected,
    ManuallyEdited,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PreferenceScope {
    Project,
    Personal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodingFeedback {
    pub id: String,
    pub task_id: String,
    pub session_id: String,
    pub kind: FeedbackKind,
    pub scope: PreferenceScope,
    pub artifact: Option<String>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl CodingFeedback {
    pub fn new(
        task_id: impl Into<String>,
        session_id: impl Into<String>,
        kind: FeedbackKind,
        scope: PreferenceScope,
        artifact: Option<String>,
        note: Option<String>,
    ) -> Result<Self, FeedbackError> {
        let task_id = task_id.into();
        let session_id = session_id.into();

        if task_id.trim().is_empty() {
            return Err(FeedbackError::EmptyTaskId);
        }
        if session_id.trim().is_empty() {
            return Err(FeedbackError::EmptySessionId);
        }
        if artifact
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(FeedbackError::EmptyArtifact);
        }

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            task_id,
            session_id,
            kind,
            scope,
            artifact,
            note,
            created_at: Utc::now(),
        })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FeedbackError {
    #[error("feedback task id cannot be empty")]
    EmptyTaskId,
    #[error("feedback session id cannot be empty")]
    EmptySessionId,
    #[error("feedback artifact cannot be empty when present")]
    EmptyArtifact,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_requires_task_and_session_identity() {
        assert_eq!(
            CodingFeedback::new(
                "",
                "session",
                FeedbackKind::Rejected,
                PreferenceScope::Project,
                None,
                None,
            )
            .unwrap_err(),
            FeedbackError::EmptyTaskId
        );

        assert_eq!(
            CodingFeedback::new(
                "task",
                "",
                FeedbackKind::Accepted,
                PreferenceScope::Personal,
                None,
                None,
            )
            .unwrap_err(),
            FeedbackError::EmptySessionId
        );
    }

    #[test]
    fn feedback_is_serializable_and_explicit() {
        let feedback = CodingFeedback::new(
            "task-1",
            "session-1",
            FeedbackKind::ManuallyEdited,
            PreferenceScope::Project,
            Some("patch-1".into()),
            Some("prefer the existing error type".into()),
        )
        .unwrap();

        let json = serde_json::to_string(&feedback).unwrap();
        assert!(json.contains("ManuallyEdited"));
        assert!(json.contains("prefer the existing error type"));
    }
}
