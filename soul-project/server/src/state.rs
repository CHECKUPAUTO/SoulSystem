use sqlx::PgPool;
use std::sync::Arc;
use crate::streaming::StreamBroker;
use crate::llm::LlmClient;
use axum::extract::FromRef;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub broker: Arc<StreamBroker>,
    pub budget_config: BudgetConfig,
    pub llm: LlmClient,
}

#[derive(Clone)]
pub struct BudgetConfig {
    pub reserved_response: usize,
    pub reserved_meta: usize,
}

impl AppState {
    pub fn new(db: PgPool, llm: LlmClient) -> Self {
        Self {
            db,
            broker: Arc::new(StreamBroker::new()),
            budget_config: BudgetConfig {
                reserved_response: 4096,
                reserved_meta: 500,
            },
            llm,
        }
    }
}

impl FromRef<AppState> for PgPool {
    fn from_ref(app_state: &AppState) -> Self {
        app_state.db.clone()
    }
}

impl FromRef<AppState> for Arc<StreamBroker> {
    fn from_ref(app_state: &AppState) -> Self {
        app_state.broker.clone()
    }
}
