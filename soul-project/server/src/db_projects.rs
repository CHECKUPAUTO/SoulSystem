use shared::{ProjectSummary, ProjectDetail, Project, ProjectKnowledge, Conversation, ContextUsage};
use uuid::Uuid;
use crate::AppState;
use crate::error::AppError;

impl AppState {
    pub async fn list_projects(&self, user_id: Uuid) -> Result<Vec<ProjectSummary>, AppError> {
        let projects = sqlx::query_as::<_, ProjectSummary>(
            "SELECT p.id, p.name, p.description, p.updated_at,
             COUNT(DISTINCT k.id) as knowledge_count,
             COUNT(DISTINCT c.id) as conversation_count
             FROM projects p
             LEFT JOIN project_knowledge k ON k.project_id = p.id
             LEFT JOIN conversations c ON c.project_id = p.id
             WHERE p.user_id = $1
             GROUP BY p.id
             ORDER BY p.updated_at DESC"
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await?;

        Ok(projects)
    }

    pub async fn get_project(&self, project_id: Uuid, user_id: Uuid) -> Result<ProjectDetail, AppError> {
        let project = sqlx::query_as::<_, Project>(
            "SELECT * FROM projects WHERE id = $1 AND user_id = $2"
        )
        .bind(project_id)
        .bind(user_id)
        .fetch_one(&self.db)
        .await?;

        let knowledge = sqlx::query_as::<_, ProjectKnowledge>(
            "SELECT id, project_id, kind, name, token_count, size_bytes, created_at
             FROM project_knowledge WHERE project_id = $1 ORDER BY position, created_at"
        )
        .bind(project_id)
        .fetch_all(&self.db)
        .await?;

        let conversations = sqlx::query_as::<_, Conversation>(
            "SELECT * FROM conversations WHERE project_id = $1 ORDER BY updated_at DESC"
        )
        .bind(project_id)
        .fetch_all(&self.db)
        .await?;

        let used_tokens: i64 = knowledge.iter().map(|k| k.token_count as i64).sum();

        Ok(ProjectDetail {
            id: project.id,
            name: project.name,
            description: project.description,
            custom_instructions: project.custom_instructions,
            model: project.model,
            knowledge,
            conversations,
            context_usage: ContextUsage {
                used_tokens,
                budget_tokens: project.context_budget_tokens,
                percent: (used_tokens as f64 / project.context_budget_tokens as f64) * 100.0,
            },
        })
    }
}
