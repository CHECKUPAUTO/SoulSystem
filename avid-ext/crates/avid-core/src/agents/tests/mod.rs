use crate::agents::base::AgentBase;
use crate::agents::infra_agent::InfraAgent;
use crate::agents::sql_agent::SqlAgent;
use crate::integrations::kubernetes::MockKubernetesClient;
use crate::integrations::database::MockDatabaseClient;
use crate::models::{Plan, Step};
use crate::llm::LlmClient;
use crate::config::Config;
use std::sync::Arc;

fn setup_agent_base() -> AgentBase {
    let config = Arc::new(Config::default());
    let llm = LlmClient::new(config.clone());
    AgentBase::new(config, llm)
}

#[tokio::test]
async fn test_infra_agent_with_mock() {
    let agent = InfraAgent::new(setup_agent_base());
    let mock_k8s = MockKubernetesClient {
        response: "Pod: test-pod, Status: Running".to_string(),
    };

    let plan = Plan {
        goal: "diagnose cluster health".to_string(),
        decomposition: vec![Step {
            description: "check pods".to_string(),
            output: "health report".to_string(),
        }],
        constraints: vec![],
        success_criteria: vec!["pods are running".to_string()],
    };

    let result = agent.execute_with_provider(&plan, None, Some(&mock_k8s)).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_sql_agent_with_mock() {
    let agent = SqlAgent::new(setup_agent_base());
    let mock_db = MockDatabaseClient {
        schema_response: "Table: users, Columns: id, name".to_string(),
    };

    let plan = Plan {
        goal: "query users table".to_string(),
        decomposition: vec![Step {
            description: "select all".to_string(),
            output: "sql script".to_string(),
        }],
        constraints: vec![],
        success_criteria: vec!["valid sql produced".to_string()],
    };

    let result = agent.execute_with_provider(&plan, None, Some(&mock_db)).await;
    assert!(result.is_err());
}
