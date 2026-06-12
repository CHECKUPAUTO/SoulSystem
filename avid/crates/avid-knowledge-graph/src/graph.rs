//! Knowledge graph — the core graph data structure for AVID.
//!
//! Backed by SQLite and compatible with the existing `MemoryStore` (same database file,
//! separate tables). Provides full CRUD for entities and relationships, graph traversal
//! (BFS), shortest path finding, contradiction detection, cluster detection, and more.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use rusqlite::params;
use tracing::debug;

pub use crate::entity::{Entity, EntityType};
pub use crate::extractor::extract_from_text;
use crate::extractor::ExtractionError;
pub use crate::relationship::{RelType, Relationship};
pub use crate::stats::GraphStats;

/// Errors that the knowledge graph can produce.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    /// Database-level error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Entity or relationship constraint violation.
    #[error("not found: {0}")]
    NotFound(String),

    /// JSON (de)serialization error.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Entity already exists with that id.
    #[error("duplicate entity: {0}")]
    DuplicateEntity(String),

    /// Extraction via LLM failed.
    #[error("extraction error: {0}")]
    Extraction(#[from] ExtractionError),
}

/// Context extracted for a specific entity — the entity plus all its relationships
/// and related entities, useful for providing rich context to an LLM agent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeContext {
    /// The central entity.
    pub entity: Entity,
    /// Directly related entities with the relationships linking them.
    pub related: Vec<(Entity, Relationship)>,
    /// Entities that this entity depends on (via DependsOn edges).
    pub dependencies: Vec<Entity>,
    /// Entities that depend on this entity.
    pub dependents: Vec<Entity>,
}

/// The AVID knowledge graph backed by SQLite.
///
/// Designed to coexist with `MemoryStore` in the same database file but using
/// separate tables (`kg_entities` and `kg_relationships`).
pub struct KnowledgeGraph {
    conn: rusqlite::Connection,
}

impl KnowledgeGraph {
    /// Open (or create) a knowledge graph at the given database path.
    ///
    /// The schema is created automatically if it doesn't exist. This is
    /// compatible with the `MemoryStore` database — the table prefix `kg_`
    /// ensures no conflicts.
    ///
    /// # Errors
    ///
    /// Returns `GraphError` if the database cannot be opened or the schema
    /// cannot be initialized.
    pub fn new(db_path: &Path) -> Result<Self, GraphError> {
        let conn = rusqlite::Connection::open(db_path)?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kg_entities(
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                entity_type TEXT NOT NULL CHECK(entity_type IN ('concept','tool','task','pattern','agent','document')),
                properties_json TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                weight REAL NOT NULL DEFAULT 0.5 CHECK(weight >= 0.0 AND weight <= 1.0)
            );

            CREATE TABLE IF NOT EXISTS kg_relationships(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id TEXT NOT NULL REFERENCES kg_entities(id) ON DELETE CASCADE,
                target_id TEXT NOT NULL REFERENCES kg_entities(id) ON DELETE CASCADE,
                rel_type TEXT NOT NULL CHECK(rel_type IN ('depends_on','implements','uses','similar_to','contradicts','precedes','contains')),
                weight REAL NOT NULL DEFAULT 0.5 CHECK(weight >= 0.0 AND weight <= 1.0),
                metadata_json TEXT NOT NULL DEFAULT '{}',
                UNIQUE(source_id, target_id, rel_type)
            );

            CREATE INDEX IF NOT EXISTS idx_kg_entities_type ON kg_entities(entity_type);
            CREATE INDEX IF NOT EXISTS idx_kg_relationships_source ON kg_relationships(source_id);
            CREATE INDEX IF NOT EXISTS idx_kg_relationships_target ON kg_relationships(target_id);
            CREATE INDEX IF NOT EXISTS idx_kg_relationships_type ON kg_relationships(rel_type);",
        )?;

        debug!("knowledge graph initialized at {}", db_path.display());
        Ok(Self { conn })
    }

    // ---------------------------------------------------------------------------
    // Entity CRUD
    // ---------------------------------------------------------------------------

    /// Add a new entity. Returns the entity id.
    ///
    /// # Errors
    ///
    /// Returns `GraphError::DuplicateEntity` if an entity with this id already exists.
    pub fn add_entity(&self, entity: Entity) -> Result<String, GraphError> {
        let props_json = serde_json::to_string(&entity.properties)?;

        self.conn.execute(
            "INSERT INTO kg_entities (id, name, entity_type, properties_json, created_at, updated_at, weight)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entity.id,
                entity.name,
                entity.entity_type.to_string(),
                props_json,
                entity.created_at,
                entity.updated_at,
                entity.weight
            ],
        ).map_err(|e| {
            if e.to_string().contains("UNIQUE constraint") || e.to_string().contains("PRIMARY KEY") {
                GraphError::DuplicateEntity(entity.id.clone())
            } else {
                GraphError::Sqlite(e)
            }
        })?;

        Ok(entity.id)
    }

    /// Retrieve an entity by id.
    ///
    /// Returns `Ok(None)` if not found.
    pub fn get_entity(&self, id: &str) -> Result<Option<Entity>, GraphError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, entity_type, properties_json, created_at, updated_at, weight FROM kg_entities WHERE id = ?1")?;

        let mut rows = stmt.query_map(params![id], |row| {
            let props_str: String = row.get(3)?;
            let props: HashMap<String, String> =
                serde_json::from_str(&props_str).unwrap_or_default();
            Ok(Entity {
                id: row.get(0)?,
                name: row.get(1)?,
                entity_type: EntityType::parse(&row.get::<_, String>(2)?)
                    .unwrap_or(EntityType::Concept),
                properties: props,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                weight: row.get(6)?,
            })
        })?;

        match rows.next() {
            Some(Ok(entity)) => Ok(Some(entity)),
            Some(Err(e)) => Err(GraphError::Sqlite(e)),
            None => Ok(None),
        }
    }

    /// Update an existing entity. All fields are overwritten.
    ///
    /// # Errors
    ///
    /// Returns `GraphError::NotFound` if no entity with this id exists.
    pub fn update_entity(&self, entity: Entity) -> Result<(), GraphError> {
        let props_json = serde_json::to_string(&entity.properties)?;
        let updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let affected = self.conn.execute(
            "UPDATE kg_entities SET name=?2, entity_type=?3, properties_json=?4, updated_at=?5, weight=?6 WHERE id=?1",
            params![
                entity.id,
                entity.name,
                entity.entity_type.to_string(),
                props_json,
                updated_at,
                entity.weight
            ],
        )?;

        if affected == 0 {
            return Err(GraphError::NotFound(entity.id));
        }
        Ok(())
    }

    /// Delete an entity and all its relationships (cascading).
    ///
    /// # Errors
    ///
    /// Returns `GraphError::NotFound` if no entity with this id exists.
    pub fn delete_entity(&self, id: &str) -> Result<(), GraphError> {
        let affected = self
            .conn
            .execute("DELETE FROM kg_entities WHERE id = ?1", params![id])?;
        if affected == 0 {
            return Err(GraphError::NotFound(id.to_string()));
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Relationship CRUD
    // ---------------------------------------------------------------------------

    /// Add a relationship between two entities.
    ///
    /// Both entities must already exist in the graph.
    ///
    /// # Errors
    ///
    /// Returns `GraphError::Sqlite` on constraint violation (missing entity or duplicate).
    pub fn add_relationship(&self, rel: &Relationship) -> Result<(), GraphError> {
        let metadata_json = serde_json::to_string(&rel.metadata)?;

        self.conn.execute(
            "INSERT INTO kg_relationships (source_id, target_id, rel_type, weight, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                rel.source_id,
                rel.target_id,
                rel.rel_type.to_string(),
                rel.weight,
                metadata_json
            ],
        )?;

        Ok(())
    }

    /// Remove a specific relationship.
    ///
    /// # Errors
    ///
    /// Returns `GraphError::NotFound` if the relationship doesn't exist.
    pub fn remove_relationship(
        &self,
        source: &str,
        target: &str,
        rel_type: RelType,
    ) -> Result<(), GraphError> {
        let affected = self.conn.execute(
            "DELETE FROM kg_relationships WHERE source_id=?1 AND target_id=?2 AND rel_type=?3",
            params![source, target, rel_type.to_string()],
        )?;

        if affected == 0 {
            return Err(GraphError::NotFound(format!(
                "relationship {source} --{rel_type}--> {target}"
            )));
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Queries
    // ---------------------------------------------------------------------------

    /// Find all entities related to `entity_id` via BFS up to `max_depth` edges.
    ///
    /// Returns entities ordered by distance from the source (closest first),
    /// including the source entity itself.
    pub fn find_related(&self, entity_id: &str, max_depth: u8) -> Result<Vec<Entity>, GraphError> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut result: Vec<Entity> = Vec::new();
        let mut queue: VecDeque<(String, u8)> = VecDeque::new();

        queue.push_back((entity_id.to_string(), 0));
        visited.insert(entity_id.to_string());

        while let Some((current_id, depth)) = queue.pop_front() {
            if let Some(entity) = self.get_entity(&current_id)? {
                result.push(entity);
            }

            if depth < max_depth {
                let neighbors = self.get_neighbors(&current_id)?;
                for nid in neighbors {
                    if visited.insert(nid.clone()) {
                        queue.push_back((nid, depth + 1));
                    }
                }
            }
        }

        Ok(result)
    }

    /// Search entities by name (case-insensitive LIKE) and optional type filter.
    pub fn search(
        &self,
        query: &str,
        entity_type: Option<EntityType>,
    ) -> Result<Vec<Entity>, GraphError> {
        let pattern = format!("%{query}%");
        let mut results = Vec::new();

        let (sql, param_vals): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(et) = entity_type {
                (
                    "SELECT id, name, entity_type, properties_json, created_at, updated_at, weight \
                     FROM kg_entities WHERE name LIKE ?1 AND entity_type = ?2 ORDER BY weight DESC",
                    vec![Box::new(pattern), Box::new(et.to_string())],
                )
            } else {
                (
                    "SELECT id, name, entity_type, properties_json, created_at, updated_at, weight \
                     FROM kg_entities WHERE name LIKE ?1 ORDER BY weight DESC",
                    vec![Box::new(pattern)],
                )
            };

        let mut stmt = self.conn.prepare(sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_vals.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let props_str: String = row.get(3)?;
            let props: HashMap<String, String> =
                serde_json::from_str(&props_str).unwrap_or_default();
            Ok(Entity {
                id: row.get(0)?,
                name: row.get(1)?,
                entity_type: EntityType::parse(&row.get::<_, String>(2)?)
                    .unwrap_or(EntityType::Concept),
                properties: props,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                weight: row.get(6)?,
            })
        })?;

        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Find the shortest path between two entities using BFS.
    ///
    /// Returns the sequence of relationships forming the path. Returns an empty
    /// vector if no path exists.
    pub fn find_path(&self, from: &str, to: &str) -> Result<Vec<Relationship>, GraphError> {
        // BFS with path tracking
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, Vec<Relationship>)> = VecDeque::new();

        visited.insert(from.to_string());
        queue.push_back((from.to_string(), Vec::new()));

        while let Some((current, path)) = queue.pop_front() {
            if current == to {
                return Ok(path);
            }

            let neighbors = self.get_relationships_from(&current)?;
            for (target, rel) in neighbors {
                if visited.insert(target.clone()) {
                    let mut new_path = path.clone();
                    new_path.push(rel);
                    queue.push_back((target, new_path));
                }
            }
        }

        Ok(Vec::new())
    }

    /// Get full context for an entity — the entity itself, all directly related
    /// entities, and dependency information.
    pub fn get_context(&self, entity_id: &str) -> Result<KnowledgeContext, GraphError> {
        let entity = self
            .get_entity(entity_id)?
            .ok_or_else(|| GraphError::NotFound(entity_id.to_string()))?;

        let related = self.get_related_pairs(entity_id)?;
        let dependencies = self.find_dependencies(entity_id)?;
        let dependents = self.what_depends_on(entity_id)?;

        Ok(KnowledgeContext {
            entity,
            related,
            dependencies,
            dependents,
        })
    }

    // ---------------------------------------------------------------------------
    // Analysis
    // ---------------------------------------------------------------------------

    /// Find all pairs of entities connected by a Contradicts relationship.
    pub fn find_contradictions(&self) -> Result<Vec<(Entity, Entity, Relationship)>, GraphError> {
        let mut results = Vec::new();

        let mut stmt = self.conn.prepare(
            "SELECT source_id, target_id, weight, metadata_json FROM kg_relationships WHERE rel_type = 'contradicts'",
        )?;

        let rows = stmt.query_map([], |row| {
            let metadata_str: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f32>(2)?,
                serde_json::from_str::<HashMap<String, String>>(&metadata_str).unwrap_or_default(),
            ))
        })?;

        for row in rows {
            let (src_id, tgt_id, weight, metadata) = row?;
            if let (Some(src), Some(tgt)) = (self.get_entity(&src_id)?, self.get_entity(&tgt_id)?) {
                let rel = Relationship {
                    source_id: src_id,
                    target_id: tgt_id,
                    rel_type: RelType::Contradicts,
                    weight,
                    metadata,
                };
                results.push((src, tgt, rel));
            }
        }

        Ok(results)
    }

    /// Detect clusters using a simple connected-components algorithm on the
    /// undirected version of the graph. Returns clusters (lists of entity ids).
    pub fn detect_clusters(&self) -> Result<Vec<Vec<String>>, GraphError> {
        // Build adjacency list (undirected)
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();

        // Load all entities
        let mut stmt = self.conn.prepare("SELECT id FROM kg_entities")?;
        let entity_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for eid in &entity_ids {
            adj.entry(eid.clone()).or_default();
        }

        // Load all relationships as undirected edges
        let mut stmt = self
            .conn
            .prepare("SELECT source_id, target_id FROM kg_relationships")?;
        let edges: Vec<(String, String)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        for (src, tgt) in edges {
            adj.entry(src.clone()).or_default().push(tgt.clone());
            adj.entry(tgt).or_default().push(src);
        }

        // DFS to find connected components
        let mut visited: HashSet<String> = HashSet::new();
        let mut clusters: Vec<Vec<String>> = Vec::new();

        for eid in &entity_ids {
            if visited.contains(eid) {
                continue;
            }
            let mut cluster = Vec::new();
            let mut stack = vec![eid.clone()];
            while let Some(node) = stack.pop() {
                if visited.insert(node.clone()) {
                    cluster.push(node.clone());
                    if let Some(neighbors) = adj.get(&node) {
                        for n in neighbors {
                            if !visited.contains(n) {
                                stack.push(n.clone());
                            }
                        }
                    }
                }
            }
            if !cluster.is_empty() {
                clusters.push(cluster);
            }
        }

        Ok(clusters)
    }

    /// Find all entities that `entity_id` depends on (outgoing DependsOn edges).
    pub fn find_dependencies(&self, entity_id: &str) -> Result<Vec<Entity>, GraphError> {
        let mut results = Vec::new();

        let mut stmt = self.conn.prepare(
            "SELECT target_id FROM kg_relationships WHERE source_id = ?1 AND rel_type = 'depends_on'",
        )?;

        let rows: Vec<String> = stmt
            .query_map(params![entity_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for target_id in rows {
            if let Some(entity) = self.get_entity(&target_id)? {
                results.push(entity);
            }
        }

        // Sort by weight descending (most important first)
        results.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(results)
    }

    /// Find all entities that depend on `entity_id` (incoming DependsOn edges).
    pub fn what_depends_on(&self, entity_id: &str) -> Result<Vec<Entity>, GraphError> {
        let mut results = Vec::new();

        let mut stmt = self.conn.prepare(
            "SELECT source_id FROM kg_relationships WHERE target_id = ?1 AND rel_type = 'depends_on'",
        )?;

        let rows: Vec<String> = stmt
            .query_map(params![entity_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for source_id in rows {
            if let Some(entity) = self.get_entity(&source_id)? {
                results.push(entity);
            }
        }

        results.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(results)
    }

    // ---------------------------------------------------------------------------
    // Stats
    // ---------------------------------------------------------------------------

    /// Compute summary statistics for the graph.
    pub fn stats(&self) -> Result<GraphStats, GraphError> {
        let total_entities: u64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM kg_entities", [], |row| row.get(0))?;

        let total_relationships: u64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM kg_relationships", [], |row| {
                    row.get(0)
                })?;

        let mut stmt = self
            .conn
            .prepare("SELECT entity_type, COUNT(*) as cnt FROM kg_entities GROUP BY entity_type")?;
        let entity_types: HashMap<String, u64> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(GraphStats::compute(
            total_entities,
            total_relationships,
            entity_types,
        ))
    }

    /// Extract entities and relationships from text using an LLM, and
    /// automatically add them to this graph.
    pub fn extract_and_store(
        &self,
        llm: &avid_core::llm::LlmClient,
        text: &str,
    ) -> Result<Vec<(Entity, Vec<Relationship>)>, GraphError> {
        let extracted = extract_from_text(llm, text)?;

        for (entity, relationships) in &extracted {
            // Skip if already exists
            if self.get_entity(&entity.id)?.is_some() {
                continue;
            }
            self.add_entity(entity.clone())?;
            for rel in relationships {
                let _ = self.add_relationship(rel);
            }
        }

        Ok(extracted)
    }

    /// Get the underlying database connection (for advanced usage).
    #[allow(dead_code)]
    pub(crate) fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }

    // ---------------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------------

    /// Get all immediate neighbor entity ids (both directions).
    fn get_neighbors(&self, entity_id: &str) -> Result<Vec<String>, GraphError> {
        let mut neighbors = Vec::new();

        // Outgoing
        let mut stmt = self
            .conn
            .prepare("SELECT target_id FROM kg_relationships WHERE source_id = ?1")?;
        let rows: Vec<String> = stmt
            .query_map(params![entity_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        neighbors.extend(rows);

        // Incoming
        let mut stmt = self
            .conn
            .prepare("SELECT source_id FROM kg_relationships WHERE target_id = ?1")?;
        let rows: Vec<String> = stmt
            .query_map(params![entity_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        neighbors.extend(rows);

        Ok(neighbors)
    }

    /// Get all outgoing relationships from an entity with target ids.
    fn get_relationships_from(
        &self,
        entity_id: &str,
    ) -> Result<Vec<(String, Relationship)>, GraphError> {
        let mut results = Vec::new();

        let mut stmt = self.conn.prepare(
            "SELECT target_id, rel_type, weight, metadata_json FROM kg_relationships WHERE source_id = ?1",
        )?;

        let rows = stmt.query_map(params![entity_id], |row| {
            let metadata_str: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f32>(2)?,
                serde_json::from_str::<HashMap<String, String>>(&metadata_str).unwrap_or_default(),
            ))
        })?;

        for row in rows {
            let (target_id, rel_type_str, weight, metadata) = row?;
            let rel_type = RelType::parse(&rel_type_str).unwrap_or(RelType::Uses);
            let rel = Relationship {
                source_id: entity_id.to_string(),
                target_id: target_id.clone(),
                rel_type,
                weight,
                metadata,
            };
            results.push((target_id, rel));
        }

        Ok(results)
    }

    /// Get all directly related entities along with the relationship.
    fn get_related_pairs(
        &self,
        entity_id: &str,
    ) -> Result<Vec<(Entity, Relationship)>, GraphError> {
        let mut results = Vec::new();

        // Outgoing
        let mut stmt = self.conn.prepare(
            "SELECT r.target_id, r.rel_type, r.weight, r.metadata_json \
             FROM kg_relationships r WHERE r.source_id = ?1",
        )?;
        let rows = stmt.query_map(params![entity_id], |row| {
            let metadata_str: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f32>(2)?,
                serde_json::from_str::<HashMap<String, String>>(&metadata_str).unwrap_or_default(),
            ))
        })?;

        for row in rows {
            let (target_id, rel_type_str, weight, metadata) = row?;
            let rel_type = RelType::parse(&rel_type_str).unwrap_or(RelType::Uses);
            if let Some(target_entity) = self.get_entity(&target_id)? {
                let rel = Relationship {
                    source_id: entity_id.to_string(),
                    target_id,
                    rel_type,
                    weight,
                    metadata,
                };
                results.push((target_entity, rel));
            }
        }

        // Incoming
        let mut stmt = self.conn.prepare(
            "SELECT r.source_id, r.rel_type, r.weight, r.metadata_json \
             FROM kg_relationships r WHERE r.target_id = ?1",
        )?;
        let rows = stmt.query_map(params![entity_id], |row| {
            let metadata_str: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f32>(2)?,
                serde_json::from_str::<HashMap<String, String>>(&metadata_str).unwrap_or_default(),
            ))
        })?;

        for row in rows {
            let (source_id, rel_type_str, weight, metadata) = row?;
            let rel_type = RelType::parse(&rel_type_str).unwrap_or(RelType::Uses);
            if let Some(source_entity) = self.get_entity(&source_id)? {
                let rel = Relationship {
                    source_id,
                    target_id: entity_id.to_string(),
                    rel_type,
                    weight,
                    metadata,
                };
                results.push((source_entity, rel));
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn setup() -> (KnowledgeGraph, NamedTempFile) {
        let tmp = NamedTempFile::new().unwrap();
        let kg = KnowledgeGraph::new(tmp.path()).unwrap();
        (kg, tmp)
    }

    fn sample_entity(name: &str, etype: EntityType) -> Entity {
        Entity::new(name.to_string(), etype).with_weight(0.7)
    }

    #[test]
    fn add_and_get_entity() {
        let (kg, _tmp) = setup();
        let e = sample_entity("TestConcept", EntityType::Concept);
        let eid = e.id.clone();
        kg.add_entity(e).unwrap();

        let got = kg.get_entity(&eid).unwrap().unwrap();
        assert_eq!(got.name, "TestConcept");
        assert_eq!(got.entity_type, EntityType::Concept);
    }

    #[test]
    fn update_entity() {
        let (kg, _tmp) = setup();
        let mut e = sample_entity("Updatable", EntityType::Task);
        let eid = e.id.clone();
        kg.add_entity(e.clone()).unwrap();

        e.name = "UpdatedName".into();
        kg.update_entity(e).unwrap();

        let got = kg.get_entity(&eid).unwrap().unwrap();
        assert_eq!(got.name, "UpdatedName");
    }

    #[test]
    fn delete_entity() {
        let (kg, _tmp) = setup();
        let e = sample_entity("Deletable", EntityType::Pattern);
        let eid = e.id.clone();
        kg.add_entity(e).unwrap();

        kg.delete_entity(&eid).unwrap();
        assert!(kg.get_entity(&eid).unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent() {
        let (kg, _tmp) = setup();
        assert!(kg.delete_entity("nonexistent").is_err());
    }

    #[test]
    fn add_relationship() {
        let (kg, _tmp) = setup();
        let src = sample_entity("Source", EntityType::Task);
        let tgt = sample_entity("Target", EntityType::Tool);
        let src_id = src.id.clone();
        let tgt_id = tgt.id.clone();

        kg.add_entity(src).unwrap();
        kg.add_entity(tgt).unwrap();

        let rel = Relationship::new(src_id.clone(), tgt_id, RelType::DependsOn);
        kg.add_relationship(&rel).unwrap();

        let deps = kg.find_dependencies(&src_id).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "Target");
    }

    #[test]
    fn remove_relationship() {
        let (kg, _tmp) = setup();
        let src = sample_entity("S", EntityType::Task);
        let tgt = sample_entity("T", EntityType::Tool);
        let src_id = src.id.clone();
        let tgt_id = tgt.id.clone();

        kg.add_entity(src).unwrap();
        kg.add_entity(tgt).unwrap();

        let rel = Relationship::new(src_id.clone(), tgt_id.clone(), RelType::Uses);
        kg.add_relationship(&rel).unwrap();
        kg.remove_relationship(&src_id, &tgt_id, RelType::Uses)
            .unwrap();

        // Removing again should fail
        assert!(kg
            .remove_relationship(&src_id, &tgt_id, RelType::Uses)
            .is_err());
    }

    #[test]
    fn find_related_bfs() {
        let (kg, _tmp) = setup();
        let a = sample_entity("A", EntityType::Concept);
        let b = sample_entity("B", EntityType::Concept);
        let c = sample_entity("C", EntityType::Concept);
        let d = sample_entity("D", EntityType::Concept);

        let aid = a.id.clone();
        let bid = b.id.clone();
        let cid = c.id.clone();
        let did = d.id.clone();

        kg.add_entity(a).unwrap();
        kg.add_entity(b).unwrap();
        kg.add_entity(c).unwrap();
        kg.add_entity(d).unwrap();

        // a → b → c → d
        kg.add_relationship(&Relationship::new(
            aid.clone(),
            bid.clone(),
            RelType::DependsOn,
        ))
        .unwrap();
        kg.add_relationship(&Relationship::new(bid, cid.clone(), RelType::DependsOn))
            .unwrap();
        kg.add_relationship(&Relationship::new(cid, did, RelType::DependsOn))
            .unwrap();

        // depth 0 → only A
        let r0 = kg.find_related(&aid, 0).unwrap();
        assert_eq!(r0.len(), 1);

        // depth 1 → A + B
        let r1 = kg.find_related(&aid, 1).unwrap();
        assert_eq!(r1.len(), 2);

        // depth 2 → A + B + C
        let r2 = kg.find_related(&aid, 2).unwrap();
        assert_eq!(r2.len(), 3);
    }

    #[test]
    fn search_entities() {
        let (kg, _tmp) = setup();
        kg.add_entity(sample_entity("RustCompiler", EntityType::Tool))
            .unwrap();
        kg.add_entity(sample_entity("RustPattern", EntityType::Pattern))
            .unwrap();
        kg.add_entity(sample_entity("PythonScript", EntityType::Tool))
            .unwrap();

        let results = kg.search("rust", None).unwrap();
        assert_eq!(results.len(), 2);

        let results = kg.search("python", Some(EntityType::Tool)).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn find_path() {
        let (kg, _tmp) = setup();
        let a = sample_entity("A", EntityType::Concept);
        let b = sample_entity("B", EntityType::Concept);
        let c = sample_entity("C", EntityType::Concept);

        let aid = a.id.clone();
        let bid = b.id.clone();
        let cid = c.id.clone();

        kg.add_entity(a).unwrap();
        kg.add_entity(b).unwrap();
        kg.add_entity(c).unwrap();

        kg.add_relationship(&Relationship::new(
            aid.clone(),
            bid.clone(),
            RelType::Precedes,
        ))
        .unwrap();
        kg.add_relationship(&Relationship::new(bid, cid.clone(), RelType::Precedes))
            .unwrap();

        let path = kg.find_path(&aid, &cid).unwrap();
        assert_eq!(path.len(), 2);

        // No path from isolated node
        let isolated = sample_entity("Isolated", EntityType::Concept);
        kg.add_entity(isolated).unwrap();
    }

    #[test]
    fn stats() {
        let (kg, _tmp) = setup();
        kg.add_entity(sample_entity("E1", EntityType::Concept))
            .unwrap();
        kg.add_entity(sample_entity("E2", EntityType::Tool))
            .unwrap();

        let s = kg.stats().unwrap();
        assert_eq!(s.total_entities, 2);
        assert_eq!(s.total_relationships, 0);
    }

    #[test]
    fn get_context() {
        let (kg, _tmp) = setup();
        let main = sample_entity("MainTask", EntityType::Task);
        let dep = sample_entity("Dependency", EntityType::Tool);

        let mid = main.id.clone();
        let did = dep.id.clone();

        kg.add_entity(main).unwrap();
        kg.add_entity(dep).unwrap();

        kg.add_relationship(&Relationship::new(mid.clone(), did, RelType::DependsOn))
            .unwrap();

        let ctx = kg.get_context(&mid).unwrap();
        assert_eq!(ctx.entity.name, "MainTask");
        assert_eq!(ctx.dependencies.len(), 1);
        assert_eq!(ctx.dependencies[0].name, "Dependency");
        assert!(ctx.dependents.is_empty());
    }

    #[test]
    fn detect_clusters() {
        let (kg, _tmp) = setup();
        let e1 = sample_entity("E1", EntityType::Concept);
        let e2 = sample_entity("E2", EntityType::Concept);
        let e3 = sample_entity("E3", EntityType::Concept);

        let id1 = e1.id.clone();
        let id2 = e2.id.clone();
        let _id3 = e3.id.clone();

        kg.add_entity(e1).unwrap();
        kg.add_entity(e2).unwrap();
        kg.add_entity(e3).unwrap();

        // e1 → e2 (connected), e3 isolated
        kg.add_relationship(&Relationship::new(id1, id2, RelType::SimilarTo))
            .unwrap();

        let clusters = kg.detect_clusters().unwrap();
        assert_eq!(clusters.len(), 2); // {e1,e2} + {e3}

        let sizes: Vec<usize> = clusters.iter().map(Vec::len).collect();
        assert!(sizes.contains(&2));
        assert!(sizes.contains(&1));
    }

    #[test]
    fn duplicate_entity_rejected() {
        let (kg, _tmp) = setup();
        let e = sample_entity("Unique", EntityType::Concept);
        kg.add_entity(e.clone()).unwrap();
        assert!(kg.add_entity(e).is_err());
    }
}
