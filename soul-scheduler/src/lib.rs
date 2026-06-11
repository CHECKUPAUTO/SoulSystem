use chrono::{DateTime, Datelike, Duration, Local, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("Invalid cron expression: {0}")]
    InvalidCron(String),
    #[error("Task not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    Storage(String),
}

pub type SchedulerResult<T> = std::result::Result<T, SchedulerError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Active,
    Paused,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub cron_expr: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub run_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
}

#[derive(Debug, Clone)]
pub enum TaskAction {
    ConsolidateMemory,
    RunShell(String),
    LlmPrompt(String),
    Custom(String),
}

impl ScheduledTask {
    pub fn new(name: &str, cron_expr: &str, description: &str) -> Self {
        let next = compute_next_run(cron_expr);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            cron_expr: cron_expr.to_string(),
            description: description.to_string(),
            status: TaskStatus::Active,
            created_at: Utc::now(),
            last_run: None,
            next_run: next,
            run_count: 0,
            success_count: 0,
            failure_count: 0,
        }
    }

    pub fn is_due(&self) -> bool {
        match self.next_run {
            Some(next) => Utc::now() >= next,
            None => false,
        }
    }

    pub fn advance(&mut self) {
        self.last_run = Some(Utc::now());
        self.run_count += 1;
        self.next_run = compute_next_run(&self.cron_expr);
    }
}

pub struct Scheduler {
    tasks: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    on_tick: Arc<dyn Fn(&ScheduledTask) -> SchedulerResult<()> + Send + Sync>,
}

impl Scheduler {
    pub fn new<F>(on_tick: F) -> Self
    where
        F: Fn(&ScheduledTask) -> SchedulerResult<()> + Send + Sync + 'static,
    {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            on_tick: Arc::new(on_tick),
        }
    }

    pub async fn add_task(&self, task: ScheduledTask) {
        self.tasks.write().await.insert(task.id.clone(), task);
    }

    pub async fn remove_task(&self, id: &str) -> SchedulerResult<()> {
        self.tasks
            .write()
            .await
            .remove(id)
            .ok_or_else(|| SchedulerError::NotFound(id.to_string()))?;
        Ok(())
    }

    pub async fn list_tasks(&self) -> Vec<ScheduledTask> {
        self.tasks.read().await.values().cloned().collect()
    }

    pub async fn run(&self) {
        let tasks = self.tasks.clone();
        let on_tick = self.on_tick.clone();
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

        loop {
            interval.tick().await;
            let due: Vec<ScheduledTask> = {
                let lock = tasks.read().await;
                lock.values()
                    .filter(|t| t.status == TaskStatus::Active && t.is_due())
                    .cloned()
                    .collect()
            };

            for task in due {
                tracing::info!("Scheduler: executing task '{}'", task.name);
                if let Err(e) = (on_tick)(&task) {
                    tracing::error!("Scheduler: task '{}' failed: {}", task.name, e);
                    if let Some(mut t) = tasks.write().await.get_mut(&task.id) {
                        t.failure_count += 1;
                    }
                } else {
                    if let Some(mut t) = tasks.write().await.get_mut(&task.id) {
                        t.success_count += 1;
                        t.advance();
                    }
                }
            }
        }
    }
}

pub fn compute_next_run(cron_expr: &str) -> Option<DateTime<Utc>> {
    let parts: Vec<&str> = cron_expr.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }

    let minute: Vec<&str> = if parts[0] == "*" {
        vec![]
    } else {
        parts[0].split('/').collect()
    };
    let hour: Vec<&str> = if parts[1] == "*" {
        vec![]
    } else {
        parts[1].split('/').collect()
    };
    let _day_of_month = parts[2];
    let _month = parts[3];
    let _day_of_week = parts[4];

    let now = Utc::now();
    let mut candidate = now + Duration::minutes(1);
    candidate = candidate.with_second(0).unwrap_or(candidate);

    for _ in 0..1440 {
        let h = candidate.hour() as i32;
        let m = candidate.minute() as i32;

        let m_match = if minute.is_empty() {
            true
        } else if minute.len() == 2 && minute[0] == "*" {
            if let Ok(step) = minute[1].parse::<i32>() {
                m % step == 0
            } else {
                false
            }
        } else if let Ok(val) = parts[0].parse::<i32>() {
            m == val
        } else {
            false
        };

        let h_match = if hour.is_empty() {
            true
        } else if hour.len() == 2 && hour[0] == "*" {
            if let Ok(step) = hour[1].parse::<i32>() {
                h % step == 0
            } else {
                false
            }
        } else if let Ok(val) = parts[1].parse::<i32>() {
            h == val
        } else {
            false
        };

        if m_match && h_match {
            return Some(candidate);
        }

        candidate = candidate + Duration::minutes(1);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = ScheduledTask::new("test", "0 */6 * * *", "Test task");
        assert_eq!(task.name, "test");
        assert_eq!(task.status, TaskStatus::Active);
        assert!(task.next_run.is_some());
    }

    #[test]
    fn test_compute_next_run_every_hour() {
        let next = compute_next_run("0 * * * *");
        assert!(next.is_some());
    }

    #[test]
    fn test_compute_next_run_every_6_hours() {
        let next = compute_next_run("0 */6 * * *");
        assert!(next.is_some());
    }

    #[test]
    fn test_is_due() {
        let mut task = ScheduledTask::new("test", "* * * * *", "Every minute");
        task.next_run = Some(Utc::now() - Duration::seconds(10));
        assert!(task.is_due());
    }

    #[test]
    fn test_advance() {
        let mut task = ScheduledTask::new("test", "* * * * *", "Every minute");
        let old_next = task.next_run;
        task.advance();
        assert_eq!(task.run_count, 1);
        assert!(task.last_run.is_some());
        assert_ne!(task.next_run, old_next);
    }

#[test]
    fn test_add_and_list_tasks() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let scheduler = Scheduler::new(|_| Ok(()));
            scheduler
                .add_task(ScheduledTask::new("a", "0 * * * *", "A"))
                .await;
            scheduler
                .add_task(ScheduledTask::new("b", "*/30 * * * *", "B"))
                .await;
            let tasks = scheduler.list_tasks().await;
            assert_eq!(tasks.len(), 2);
        });
    }

    #[test]
    fn test_remove_task() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let scheduler = Scheduler::new(|_| Ok(()));
            let task = ScheduledTask::new("x", "0 * * * *", "X");
            let id = task.id.clone();
            scheduler.add_task(task).await;
            scheduler.remove_task(&id).await.unwrap();
            let tasks = scheduler.list_tasks().await;
            assert_eq!(tasks.len(), 0);
        });
    }
}