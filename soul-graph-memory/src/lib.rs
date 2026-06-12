use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Edge not found: {0}")]
    EdgeNotFound(String),
    #[error("Cycle detected")]
    CycleDetected,
    #[error("IO error: {0}")]
    Io(String),
    #[error("Serialization error: {0}")]
    Serde(String),
}

pub type Result<T> = std::result::Result<T, GraphError>;

// ─── Node Types ──────────────────────────────────────────────────────────────

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

// ─── Node ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub node_type: NodeType,
    pub label: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Node {
    pub fn new(node_type: NodeType, label: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            node_type,
            label: label.into(),
            content: String::new(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_content(mut self, content: &str) -> Self {
        self.content = content.into();
        self
    }

    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

// ─── Edge ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub edge_type: EdgeType,
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl Edge {
    pub fn new(source: &str, target: &str, edge_type: EdgeType) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source: source.into(),
            target: target.into(),
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

// ─── Knowledge Graph ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    nodes: HashMap<String, Node>,
    edges: HashMap<String, Edge>,
    adj_out: HashMap<String, Vec<String>>,
    adj_in: HashMap<String, Vec<String>>,
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
        self.nodes.insert(id.clone(), node);
        self.adj_out.entry(id.clone()).or_default();
        self.adj_in.entry(id.clone()).or_default();
        id
    }

    pub fn add_edge(&mut self, edge: Edge) -> Result<String> {
        if !self.nodes.contains_key(&edge.source) {
            return Err(GraphError::NodeNotFound(edge.source));
        }
        if !self.nodes.contains_key(&edge.target) {
            return Err(GraphError::NodeNotFound(edge.target));
        }

        let id = edge.id.clone();
        self.adj_out
            .entry(edge.source.clone())
            .or_default()
            .push(id.clone());
        self.adj_in
            .entry(edge.target.clone())
            .or_default()
            .push(id.clone());
        self.edges.insert(id.clone(), edge);
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
            .filter(|n| &n.node_type == node_type)
            .collect()
    }

    pub fn find_nodes_by_label(&self, label: &str) -> Vec<&Node> {
        let label_lower = label.to_lowercase();
        self.nodes
            .values()
            .filter(|n| n.label.to_lowercase().contains(&label_lower))
            .collect()
    }

    pub fn neighbors_out(&self, node_id: &str) -> Vec<&Node> {
        self.adj_out
            .get(node_id)
            .map(|edge_ids| {
                edge_ids
                    .iter()
                    .filter_map(|eid| self.edges.get(eid).and_then(|e| self.nodes.get(&e.target)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn edges_out(&self, node_id: &str) -> Vec<&Edge> {
        self.adj_out
            .get(node_id)
            .map(|edge_ids| {
                edge_ids
                    .iter()
                    .filter_map(|eid| self.edges.get(eid))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn neighbors_in(&self, node_id: &str) -> Vec<&Node> {
        self.adj_in
            .get(node_id)
            .map(|edge_ids| {
                edge_ids
                    .iter()
                    .filter_map(|eid| self.edges.get(eid).and_then(|e| self.nodes.get(&e.source)))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn edges_from(&self, node_id: &str) -> Vec<&Edge> {
        self.adj_out
            .get(node_id)
            .map(|eids| eids.iter().filter_map(|eid| self.edges.get(eid)).collect())
            .unwrap_or_default()
    }

    pub fn edges_to(&self, node_id: &str) -> Vec<&Edge> {
        self.adj_in
            .get(node_id)
            .map(|eids| eids.iter().filter_map(|eid| self.edges.get(eid)).collect())
            .unwrap_or_default()
    }

    pub fn find_path(&self, from: &str, to: &str, max_depth: usize) -> Option<Vec<String>> {
        if from == to {
            return Some(vec![from.into()]);
        }

        // Dijkstra's algorithm with edge weights
        // Use (cost_x1000, node) to avoid f64 Ord issues
        let mut distances: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        let mut prev: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut visited = std::collections::HashSet::new();

        distances.insert(from.to_string(), 0);
        let mut pq = std::collections::BinaryHeap::new();
        pq.push(std::cmp::Reverse((0u64, from.to_string())));

        while let Some(std::cmp::Reverse((cost, current))) = pq.pop() {
            if current == to {
                let mut path = vec![current.clone()];
                let mut node = current.clone();
                while let Some(p) = prev.get(&node) {
                    path.push(p.clone());
                    node = p.clone();
                }
                path.reverse();
                return Some(path);
            }

            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            if (cost / 1000) > max_depth as u64 {
                continue;
            }

            for edge in self.edges_out(&current) {
                let new_cost = cost + (edge.weight * 1000.0) as u64 + 1;
                let neighbor = &edge.target;
                if !visited.contains(neighbor) {
                    let dominated = distances.get(neighbor).is_none_or(|&old| new_cost < old);
                    if dominated {
                        distances.insert(neighbor.clone(), new_cost);
                        prev.insert(neighbor.clone(), current.clone());
                        pq.push(std::cmp::Reverse((new_cost, neighbor.clone())));
                    }
                }
            }
        }

        None
    }

    pub fn has_cycle(&self) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut stack = std::collections::HashSet::new();

        fn dfs(
            node: &str,
            graph: &KnowledgeGraph,
            visited: &mut std::collections::HashSet<String>,
            stack: &mut std::collections::HashSet<String>,
        ) -> bool {
            visited.insert(node.to_string());
            stack.insert(node.to_string());

            for neighbor in graph.neighbors_out(node) {
                if !visited.contains(&neighbor.id) {
                    if dfs(&neighbor.id, graph, visited, stack) {
                        return true;
                    }
                } else if stack.contains(&neighbor.id) {
                    return true;
                }
            }

            stack.remove(node);
            false
        }

        for node_id in self.nodes.keys() {
            if !visited.contains(node_id) && dfs(node_id, self, &mut visited, &mut stack) {
                return true;
            }
        }

        false
    }

    pub fn topological_sort(&self) -> Result<Vec<String>> {
        if self.has_cycle() {
            return Err(GraphError::CycleDetected);
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
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

        let mut sorted = Vec::new();
        while let Some(node) = queue.pop() {
            sorted.push(node.clone());
            for neighbor in self.neighbors_out(&node) {
                let deg = in_degree.get_mut(&neighbor.id).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push(neighbor.id.clone());
                }
            }
        }

        Ok(sorted)
    }

    /// Remove all edges whose `created_at` is older than the given timestamp.
    /// Returns the number of edges removed. Also cleans up adjacency indices.
    pub fn purge_stale_edges(&mut self, older_than: DateTime<Utc>) -> usize {
        let stale_ids: Vec<String> = self
            .edges
            .iter()
            .filter(|(_, edge)| edge.created_at < older_than)
            .map(|(id, _)| id.clone())
            .collect();

        let count = stale_ids.len();
        for id in &stale_ids {
            if let Some(edge) = self.edges.remove(id) {
                if let Some(out_edges) = self.adj_out.get_mut(&edge.source) {
                    out_edges.retain(|eid| eid != id);
                }
                if let Some(in_edges) = self.adj_in.get_mut(&edge.target) {
                    in_edges.retain(|eid| eid != id);
                }
            }
        }
        count
    }

    /// Remove all nodes that have zero edges (both outgoing and incoming).
    /// Returns the IDs of the removed nodes.
    pub fn purge_nodes_without_edges(&mut self) -> Vec<String> {
        let orphan_ids: Vec<String> = self
            .nodes
            .keys()
            .filter(|id| {
                let out_empty = self.adj_out.get(*id).map(|v| v.is_empty()).unwrap_or(true);
                let in_empty = self.adj_in.get(*id).map(|v| v.is_empty()).unwrap_or(true);
                out_empty && in_empty
            })
            .cloned()
            .collect();

        for id in &orphan_ids {
            self.nodes.remove(id);
            self.adj_out.remove(id);
            self.adj_in.remove(id);
        }
        orphan_ids
    }

    pub fn subgraph(&self, node_ids: &[String]) -> KnowledgeGraph {
        let mut sub = KnowledgeGraph::new();
        let id_set: std::collections::HashSet<&String> = node_ids.iter().collect();

        for node_id in node_ids {
            if let Some(node) = self.nodes.get(node_id) {
                sub.add_node(node.clone());
            }
        }

        for edge in self.edges.values() {
            if id_set.contains(&edge.source) && id_set.contains(&edge.target) {
                sub.edges.insert(edge.id.clone(), edge.clone());
            }
        }

        sub
    }

    pub fn stats(&self) -> GraphStats {
        let mut type_counts: HashMap<String, usize> = HashMap::new();
        for node in self.nodes.values() {
            let type_name = format!("{:?}", node.node_type);
            *type_counts.entry(type_name).or_insert(0) += 1;
        }

        let mut edge_type_counts: HashMap<String, usize> = HashMap::new();
        for edge in self.edges.values() {
            let type_name = format!("{:?}", edge.edge_type);
            *edge_type_counts.entry(type_name).or_insert(0) += 1;
        }

        GraphStats {
            node_count: self.nodes.len(),
            edge_count: self.edges.len(),
            type_counts,
            edge_type_counts,
            has_cycle: self.has_cycle(),
        }
    }

    /// Extract relevant context from the graph for a query string.
    /// Returns a formatted string summarizing relevant nodes and their relationships.
    pub fn context_for_query(&self, query: &str, max_nodes: usize) -> String {
        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();

        // Find nodes whose label matches any keyword
        let mut matched: Vec<(&String, &Node)> = self
            .nodes
            .iter()
            .filter(|(_, node)| {
                let label_lower = node.label.to_lowercase();
                keywords.iter().any(|kw| label_lower.contains(kw))
            })
            .collect();

        // Sort by edge count (most connected first)
        matched.sort_by_key(|(id, _)| {
            let out = self.edges_from(id).len();
            let inc = self.edges_to(id).len();
            std::cmp::Reverse(out + inc)
        });

        matched.truncate(max_nodes);

        if matched.is_empty() {
            return String::new();
        }

        let mut ctx = format!("Graph context ({} relevant nodes):\n", matched.len());
        for (id, node) in &matched {
            let mut relations = Vec::new();
            for edge in self.edges_from(id) {
                if let Some(target) = self.nodes.get(&edge.target) {
                    relations.push(format!("→ {:?} → {}", edge.edge_type, target.label));
                }
            }
            for edge in self.edges_to(id) {
                if let Some(source) = self.nodes.get(&edge.source) {
                    relations.push(format!("← {:?} ← {}", edge.edge_type, source.label));
                }
            }
            ctx.push_str(&format!(
                "  - [{}] {}: {}\n",
                &id[..8],
                node.label,
                relations.join(", ")
            ));
        }
        ctx
    }

    pub fn persist(&self, path: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).map_err(|e| GraphError::Serde(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| GraphError::Io(e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path).map_err(|e| GraphError::Io(e.to_string()))?;
        serde_json::from_str(&json).map_err(|e| GraphError::Serde(e.to_string()))
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Graph Stats ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub type_counts: HashMap<String, usize>,
    pub edge_type_counts: HashMap<String, usize>,
    pub has_cycle: bool,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_node() {
        let mut g = KnowledgeGraph::new();
        let n = Node::new(NodeType::Concept, "Rust");
        let id = g.add_node(n);
        assert!(g.get_node(&id).is_some());
    }

    #[test]
    fn test_add_edge() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Concept, "A"));
        let b = g.add_node(Node::new(NodeType::Concept, "B"));
        let e = g.add_edge(Edge::new(&a, &b, EdgeType::DependsOn));
        assert!(e.is_ok());
    }

    #[test]
    fn test_find_path() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Concept, "A"));
        let b = g.add_node(Node::new(NodeType::Concept, "B"));
        let c = g.add_node(Node::new(NodeType::Concept, "C"));
        g.add_edge(Edge::new(&a, &b, EdgeType::DependsOn)).unwrap();
        g.add_edge(Edge::new(&b, &c, EdgeType::DependsOn)).unwrap();

        let path = g.find_path(&a, &c, 10).unwrap();
        assert_eq!(path.len(), 3);
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
    fn test_topological_sort() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Concept, "A"));
        let b = g.add_node(Node::new(NodeType::Concept, "B"));
        g.add_edge(Edge::new(&a, &b, EdgeType::DependsOn)).unwrap();
        let sorted = g.topological_sort().unwrap();
        assert!(sorted.iter().position(|x| x == &a) < sorted.iter().position(|x| x == &b));
    }

    #[test]
    fn test_find_by_type() {
        let mut g = KnowledgeGraph::new();
        g.add_node(Node::new(NodeType::Concept, "A"));
        g.add_node(Node::new(NodeType::Bug, "B"));
        g.add_node(Node::new(NodeType::Concept, "C"));
        let concepts = g.find_nodes_by_type(&NodeType::Concept);
        assert_eq!(concepts.len(), 2);
    }

    #[test]
    fn test_subgraph() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Concept, "A"));
        let b = g.add_node(Node::new(NodeType::Concept, "B"));
        let c = g.add_node(Node::new(NodeType::Concept, "C"));
        g.add_edge(Edge::new(&a, &b, EdgeType::DependsOn)).unwrap();
        g.add_edge(Edge::new(&b, &c, EdgeType::DependsOn)).unwrap();

        let sub = g.subgraph(&[a.clone(), b.clone()]);
        assert_eq!(sub.nodes.len(), 2);
        assert_eq!(sub.edges.len(), 1);
    }

    #[test]
    fn test_stats() {
        let mut g = KnowledgeGraph::new();
        g.add_node(Node::new(NodeType::Concept, "A"));
        g.add_node(Node::new(NodeType::Bug, "B"));
        let stats = g.stats();
        assert_eq!(stats.node_count, 2);
        assert!(!stats.has_cycle);
    }

    #[test]
    fn test_persist_and_load() {
        let mut g = KnowledgeGraph::new();
        g.add_node(Node::new(NodeType::Concept, "Test"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.json");
        g.persist(&path).unwrap();
        let loaded = KnowledgeGraph::load(&path).unwrap();
        assert_eq!(loaded.nodes.len(), 1);
    }

    #[test]
    fn test_neighbors() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Concept, "A"));
        let b = g.add_node(Node::new(NodeType::Concept, "B"));
        g.add_edge(Edge::new(&a, &b, EdgeType::DependsOn)).unwrap();
        assert_eq!(g.neighbors_out(&a).len(), 1);
        assert_eq!(g.neighbors_in(&b).len(), 1);
    }

    #[test]
    fn test_node_metadata() {
        let n = Node::new(NodeType::Concept, "Test")
            .with_content("some content")
            .with_metadata("score", serde_json::json!(42));
        assert_eq!(n.content, "some content");
        assert_eq!(n.metadata["score"], 42);
    }

    #[test]
    fn test_context_for_query() {
        let mut g = KnowledgeGraph::new();
        let a = g.add_node(Node::new(NodeType::Task, "Fix login bug"));
        let b = g.add_node(Node::new(NodeType::Bug, "Auth timeout"));
        let _c = g.add_node(Node::new(NodeType::Concept, "Unrelated"));
        g.add_edge(Edge::new(&a, &b, EdgeType::CausedBy)).unwrap();

        let ctx = g.context_for_query("login", 5);
        assert!(ctx.contains("Fix login bug"));
        assert!(ctx.contains("Auth timeout"));
        assert!(!ctx.contains("Unrelated"));
    }

    #[test]
    fn test_context_empty() {
        let g = KnowledgeGraph::new();
        let ctx = g.context_for_query("xyz", 5);
        assert!(ctx.is_empty());
    }
}
