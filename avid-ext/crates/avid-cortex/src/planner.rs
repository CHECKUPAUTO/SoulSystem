use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub dependencies: Vec<String>,
    pub status: TaskStatus,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Logic for task decomposition and execution planning.
#[derive(Default)]
pub struct Planner {
    tasks: HashMap<String, Task>,
}

impl Planner {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_task(&mut self, task: Task) {
        self.tasks.insert(task.id.clone(), task);
    }
    pub fn get_execution_order(&self) -> Vec<String> {
        let mut order = Vec::new();
        let mut in_degree = HashMap::new();
        for task in self.tasks.values() {
            in_degree.entry(task.id.clone()).or_insert(0);
            for dep in &task.dependencies {
                *in_degree.entry(dep.clone()).or_insert(0) += 0;
                *in_degree.entry(task.id.clone()).or_insert(0) += 1;
            }
        }
        let mut queue: VecDeque<_> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();
        while let Some(id) = queue.pop_front() {
            order.push(id.clone());
            for task in self.tasks.values() {
                if task.dependencies.contains(&id) {
                    if let Some(deg) = in_degree.get_mut(&task.id) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(task.id.clone());
                        }
                    }
                }
            }
        }
        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_planner_topological_sort() {
        let mut planner = Planner::new();
        planner.add_task(Task {
            id: "1".to_string(),
            description: "Task 1".to_string(),
            dependencies: vec![],
            status: TaskStatus::Pending,
        });
        planner.add_task(Task {
            id: "2".to_string(),
            description: "Task 2".to_string(),
            dependencies: vec!["1".to_string()],
            status: TaskStatus::Pending,
        });
        assert_eq!(planner.get_execution_order(), vec!["1", "2"]);
    }
}
