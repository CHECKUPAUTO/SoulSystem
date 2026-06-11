use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GraphError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Edge not found: {0}")]
    EdgeNotFound(String),
    #[error("Cycle detected")]
    CycleDetected,
    #[error("IO error: {0}")]
    Io(String),
    #[error("Serde error: {0}")]
    Serde(String),
}

pub type GraphResult<T> = std::result::Result<T, GraphError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NodeType {
    Concept,
    Decision,
    Bug,
    Task,
    Code,
    Document,
    Person,
    Tool,
    Model,
    Error,
    Metric,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EdgeType {
    DependsOn,
    CausedBy,
    SolvedBy,
    Implements,
    RelatedTo,
    CreatedBy,
    Uses,
    Extends,
    Blocks,
    Follows,
    Triggers,
    Contains,
    PartOf,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub node_type: NodeType,
    pub label: String,
    pub content: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub edge_type: EdgeType,
    pub weight: f64,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub type_counts: HashMap<String, usize>,
    pub edge_type_counts: HashMap<String, usize>,
    pub has_cycle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    nodes: HashMap<String, Node>,
    edges: HashMap<String, Edge>,
    adj_out: HashMap<String, Vec<String>>,
    adj_in: HashMap<String, Vec<String>>,
}

impl Node {
    pub fn new(node_type: NodeType, label: &str) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            node_type,
            label: label.to_string(),
            content: String::new(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_content(mut self, content: &str) -> Self {
        self.content = content.to_string();
        self
    }

    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }
}

impl Edge {
    pub fn new(source: &str, target: &str, edge_type: EdgeType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            source: source.to_string(),
            target: target.to_string(),
            edge_type,
            weight: 1.0,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            adj_out: HashMap::new(),
            adj_in: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: Node) -> String {
        let id = node.id.clone();
        self.nodes.insert(node.id.clone(), node);
        self.adj_out.entry(id.clone()).or_default();
        self.adj_in.entry(id.clone()).or_default();
        id
    }

    pub fn add_edge(&mut self, edge: Edge) -> GraphResult<String> {
        if !self.nodes.contains_key(&edge.source) {
            return Err(GraphError::NodeNotFound(edge.source.clone()));
        }
        if !self.nodes.contains_key(&edge.target) {
            return Err(GraphError::NodeNotFound(edge.target.clone()));
        }
        let id = edge.id.clone();
        let source = edge.source.clone();
        let target = edge.target.clone();
        self.edges.insert(id.clone(), edge);
        self.adj_out
            .entry(source.clone())
            .or_default()
            .push(target.clone());
        self.adj_in
            .entry(target.clone())
            .or_default()
            .push(source);
        Ok(id)
    }

    pub fn get_node(&self, id: &str) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn get_edge(&self, id: &str) -> Option<&Edge> {
        self.edges.get(id)
    }

    pub fn find_nodes_by_type(&self, node_type: &NodeType) -> Vec<&Node> {
        self.nodes
            .values()
            .filter(|n| n.node_type == *node_type)
            .collect()
    }

    pub fn find_nodes_by_label(&self, label: &str) -> Vec<&Node> {
        let lower = label.to_lowercase();
        self.nodes
            .values()
            .filter(|n| n.label.to_lowercase().contains(&lower))
            .collect()
    }

    pub fn neighbors_out(&self, node_id: &str) -> Vec<&Node> {
        self.adj_out
            .get(node_id)
            .map(|neighbors| {
                neighbors
                    .iter()
                    .filter_map(|id| self.nodes.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn neighbors_in(&self, node_id: &str) -> Vec<&Node> {
        self.adj_in
            .get(node_id)
            .map(|neighbors| {
                neighbors
                    .iter()
                    .filter_map(|id| self.nodes.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn edges_from(&self, node_id: &str) -> Vec<&Edge> {
        self.edges
            .values()
            .filter(|e| e.source == node_id)
            .collect()
    }

    pub fn edges_to(&self, node_id: &str) -> Vec<&Edge> {
        self.edges
            .values()
            .filter(|e| e.target == node_id)
            .collect()
    }

    pub fn find_path(&self, from: &str, to: &str, max_depth: usize) -> Option<Vec<String>> {
        use std::collections::VecDeque;
        let mut queue = VecDeque::new();
        let mut visited = HashMap::new();
        queue.push_back((from.to_string(), 0u8));
        visited.insert(from.to_string(), None);

        while let Some((current, depth)) = queue.pop_front() {
            if current == to {
                let mut path = Vec::new();
                let mut node = Some(current);
                while let Some(n) = node {
                    path.push(n.clone());
                    node = visited.get(&n).and_then(|p| p.clone());
                }
                path.reverse();
                return Some(path);
            }

            if depth >= max_depth as u8 {
                continue;
            }

            if let Some(neighbors) = self.adj_out.get(&current) {
                for neighbor in neighbors {
                    if !visited.contains_key(neighbor) {
                        visited.insert(neighbor.clone(), Some(current.clone()));
                        queue.push_back((neighbor.clone(), depth + 1));
                    }
                }
            }
        }
        None
    }

    pub fn has_cycle(&self) -> bool {
        let mut visited = HashMap::new();
        for node_id in self.nodes.keys() {
            if !visited.contains_key(node_id) {
                let mut stack = Vec::new();
                if self.detect_cycle_dfs(node_id, &mut visited, &mut stack) {
                    return true;
                }
            }
        }
        false
    }

    fn detect_cycle_dfs(
        &self,
        node_id: &str,
        visited: &mut HashMap<String, bool>,
        stack: &mut Vec<String>,
    ) -> bool {
        visited.insert(node_id.to_string(), true);
        stack.push(node_id.to_string());

        if let Some(neighbors) = self.adj_out.get(node_id) {
            for neighbor in neighbors {
                if !visited.contains_key(neighbor) {
                    if self.detect_cycle_dfs(neighbor, visited, stack) {
                        return true;
                    }
                } else if stack.contains(neighbor) {
                    return true;
                }
            }
        }

        stack.pop();
        false
    }

    pub fn topological_sort(&self) -> GraphResult<Vec<String>> {
        let mut in_degree = HashMap::new();
        for node_id in self.nodes.keys() {
            in_degree.entry(node_id.clone()).or_insert(0);
        }
        for edge in self.edges.values() {
            *in_degree.entry(edge.target.clone()).or_insert(0) += 1;
        }

        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut result = Vec::new();
        while let Some(node) = queue.pop() {
            result.push(node.clone());
            if let Some(neighbors) = self.adj_out.get(&node) {
                for neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(neighbor.clone());
                        }
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            return Err(GraphError::CycleDetected);
        }
        Ok(result)
    }

    pub fn purge_stale_edges(&mut self, older_than: DateTime<Utc>) -> usize {
        let stale: Vec<String> = self
            .edges
            .iter()
            .filter(|(_, e)| e.created_at < older_than)
            .map(|(id, _)| id.clone())
            .collect();
        let count = stale.len();
        for id in stale {
            if let Some(edge) = self.edges.remove(&id) {
                if let Some(neighbors) = self.adj_out.get_mut(&edge.source) {
                    neighbors.retain(|n| n != &edge.target);
                }
                if let Some(neighbors) = self.adj_in.get_mut(&edge.target) {
                    neighbors.retain(|n| n != &edge.source);
                }
            }
        }
        count
    }

    pub fn purge_nodes_without_edges(&mut self) -> Vec<String> {
        let orphan: Vec<String> = self
            .nodes
            .keys()
            .filter(|id| {
                let out_empty = self
                    .adj_out
                    .get(*id)
                    .map(|n| n.is_empty())
                    .unwrap_or(true);
                let in_empty = self
                    .adj_in
                    .get(*id)
                    .map(|n| n.is_empty())
                    .unwrap_or(true);
                out_empty && in_empty
            })
            .cloned()
            .collect();
        for id in &orphan {
            self.nodes.remove(id);
            self.adj_out.remove(id);
            self.adj_in.remove(id);
        }
        orphan
    }

    pub fn subgraph(&self, node_ids: &[String]) -> KnowledgeGraph {
        let mut sub = KnowledgeGraph::new();
        for id in node_ids {
            if let Some(node) = self.nodes.get(id) {
                sub.add_node(node.clone());
            }
        }
        for id in node_ids {
            if let Some(edges) = self.adj_out.get(id) {
                for target in edges {
                    if node_ids.contains(target) {
                        if let Some(edge) = self
                            .edges
                            .values()
                            .find(|e| e.source == *id && e.target == *target)
                        {
                            let _ = sub.add_edge(edge.clone());
                        }
                    }
                }
            }
        }
        sub
    }

    pub fn stats(&self) -> GraphStats {
        let mut type_counts = HashMap::new();
        let mut edge_type_counts = HashMap::new();
        for node in self.nodes.values() {
            let key = format!("{:?}", node.node_type);
            *type_counts.entry(key).or_insert(0) += 1;
        }
        for edge in self.edges.values() {
            let key = format!("{:?}", edge.edge_type);
            *edge_type_counts.entry(key).or_insert(0) += 1;
        }
        GraphStats {
            node_count: self.nodes.len(),
            edge_count: self.edges.len(),
            type_counts,
            edge_type_counts,
            has_cycle: self.has_cycle(),
        }
    }

    pub fn context_for_query(&self, query: &str, max_nodes: usize) -> String {
        let lower = query.to_lowercase();
        let mut matched: Vec<&Node> = self
            .nodes
            .values()
            .filter(|n| {
                n.label.to_lowercase().contains(&lower)
                    || n.content.to_lowercase().contains(&lower)
            })
            .collect();
        matched.sort_by(|a, b| {
            let a_score = if a.label.to_lowercase().contains(&lower) {
                2
            } else {
                1
            };
            let b_score = if b.label.to_lowercase().contains(&lower) {
                2
            } else {
                1
            };
            b_score.cmp(&a_score)
        });
        matched.truncate(max_nodes);

        let mut parts = Vec::new();
        for node in &matched {
            parts.push(format!(
                "[{}] {}: {}",
                format!("{:?}", node.node_type),
                node.label,
                node.content
            ));
            let rel: Vec<String> = self
                .edges
                .values()
                .filter(|e| e.source == node.id || e.target == node.id)
                .map(|e| {
                    let other = if e.source == node.id {
                        &e.target
                    } else {
                        &e.source
                    };
                    let other_label = self
                        .nodes
                        .get(other)
                        .map(|n| n.label.as_str())
                        .unwrap_or(other);
                    format!("  --{:?}--> {}", e.edge_type, other_label)
                })
                .collect();
            parts.extend(rel);
        }
        parts.join("\n")
    }

    pub fn persist(&self, path: &Path) -> GraphResult<()> {
        let data = serde_json::to_string(self).map_err(|e| GraphError::Serde(e.to_string()))?;
        std::fs::write(path, data).map_err(|e| GraphError::Io(e.to_string()))?;
        Ok(())
    }

    pub fn load(path: &Path) -> GraphResult<Self> {
        let data = std::fs::read_to_string(path).map_err(|e| GraphError::Io(e.to_string()))?;
        serde_json::from_str(&data).map_err(|e| GraphError::Serde(e.to_string()))
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_node() {
        let mut g = KnowledgeGraph::new();
        let node = Node::new(NodeType::Concept, "test");
        let id = g.add_node(node);
        assert!(g.get_node(&id).is_some());
    }

    #[test]
    fn test_add_edge() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Concept, "A"));
        let b = g.add_node(Node::new(NodeType::Concept, "B"));
        let edge = Edge::new(&a, &b, EdgeType::DependsOn);
        assert!(g.add_edge(edge).is_ok());
    }

    #[test]
    fn test_edge_to_nonexistent_node() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Concept, "A"));
        let edge = Edge::new(&a, "nonexistent", EdgeType::DependsOn);
        assert!(g.add_edge(edge).is_err());
    }

    #[test]
    fn test_pathfinding() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Concept, "A"));
        let b = g.add_node(Node::new(NodeType::Concept, "B"));
        let c = g.add_node(Node::new(NodeType::Concept, "C"));
        g.add_edge(Edge::new(&a, &b, EdgeType::DependsOn)).unwrap();
        g.add_edge(Edge::new(&b, &c, EdgeType::DependsOn)).unwrap();
        let path = g.find_path(&a, &c, 10);
        assert!(path.is_some());
        assert_eq!(path.unwrap().len(), 3);
    }

    #[test]
    fn test_cycle_detection() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Concept, "A"));
        let b = g.add_node(Node::new(NodeType::Concept, "B"));
        g.add_edge(Edge::new(&a, &b, EdgeType::DependsOn)).unwrap();
        g.add_edge(Edge::new(&b, &a, EdgeType::DependsOn)).unwrap();
        assert!(g.has_cycle());
    }

    #[test]
    fn test_persist_roundtrip() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Concept, "A"));
        let b = g.add_node(Node::new(NodeType::Concept, "B"));
        g.add_edge(Edge::new(&a, &b, EdgeType::DependsOn)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.json");
        g.persist(&path).unwrap();
        let loaded = KnowledgeGraph::load(&path).unwrap();
        assert_eq!(loaded.nodes.len(), g.nodes.len());
    }

    #[test]
    fn test_context_for_query() {
        let mut g = KnowledgeGraph::new();
        let n = Node::new(NodeType::Decision, "Use Rust for core").with_content("Rust is safe");
        g.add_node(n);
        let ctx = g.context_for_query("Rust", 5);
        assert!(ctx.contains("Rust"));
    }
}