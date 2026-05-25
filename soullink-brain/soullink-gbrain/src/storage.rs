/*
 * AVID Standard Header
 * Project: SoulLink
 * Module: soullink-gbrain
 * Author: SoulLink Team
 */

//! SQLite storage for entities and edges.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub entity_type: String,
    pub name: String,
    pub source_page: String,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub weight: f32,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn: Arc::new(Mutex::new(conn)) };
        db.init()?;
        Ok(db)
    }

    pub fn init(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS entities (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                name TEXT NOT NULL,
                source_page TEXT,
                confidence REAL,
                created_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS edges (
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                relation_type TEXT NOT NULL,
                weight REAL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (source_id, target_id, relation_type),
                FOREIGN KEY(source_id) REFERENCES entities(id),
                FOREIGN KEY(target_id) REFERENCES entities(id)
            )",
            [],
        )?;

        // Bolt ⚡: Index for faster graph-boost lookups where entity is the target.
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_edges_target_id ON edges(target_id)",
            [],
        )?;

        Ok(())
    }

    pub fn insert_entity(&self, entity: &Entity) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO entities (id, type, name, source_page, confidence, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entity.id,
                entity.entity_type,
                entity.name,
                entity.source_page,
                entity.confidence,
                entity.created_at.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn insert_edge(&self, edge: &Edge) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO edges (source_id, target_id, relation_type, weight, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                edge.source_id,
                edge.target_id,
                edge.relation_type,
                edge.weight,
                edge.created_at.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn get_entities(&self) -> Result<Vec<Entity>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, type, name, source_page, confidence, created_at FROM entities")?;
        let rows = stmt.query_map([], |row| {
            let created_at_str: String = row.get(5)?;
            Ok(Entity {
                id: row.get(0)?,
                entity_type: row.get(1)?,
                name: row.get(2)?,
                source_page: row.get(3)?,
                confidence: row.get(4)?,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| Utc::now().into())
                    .with_timezone(&Utc),
            })
        })?;

        let mut entities = Vec::new();
        for row in rows {
            entities.push(row?);
        }
        Ok(entities)
    }

    // Bolt ⚡: O(1) lookup by ID instead of O(N) scan.
    pub fn get_entity_by_id(&self, id: &str) -> Result<Option<Entity>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, type, name, source_page, confidence, created_at FROM entities WHERE id = ?1")?;
        let entity = stmt.query_row(params![id], |row| {
            let created_at_str: String = row.get(5)?;
            Ok(Entity {
                id: row.get(0)?,
                entity_type: row.get(1)?,
                name: row.get(2)?,
                source_page: row.get(3)?,
                confidence: row.get(4)?,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| Utc::now().into())
                    .with_timezone(&Utc),
            })
        }).optional()?;
        Ok(entity)
    }

    pub fn get_edges_for_entity(&self, entity_id: &str) -> Result<Vec<Edge>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT source_id, target_id, relation_type, weight, created_at
             FROM edges WHERE source_id = ?1 OR target_id = ?1"
        )?;
        let rows = stmt.query_map(params![entity_id], |row| {
            let created_at_str: String = row.get(4)?;
            Ok(Edge {
                source_id: row.get(0)?,
                target_id: row.get(1)?,
                relation_type: row.get(2)?,
                weight: row.get(3)?,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| Utc::now().into())
                    .with_timezone(&Utc),
            })
        })?;

        let mut edges = Vec::new();
        for row in rows {
            edges.push(row?);
        }
        Ok(edges)
    }

    // Bolt ⚡: Faster edge counting without loading all data.
    pub fn get_edge_count_for_entity(&self, id: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM edges WHERE source_id = ?1 OR target_id = ?1")?;
        let count: usize = stmt.query_row(params![id], |row| row.get(0))?;
        Ok(count)
    }

    /// Bolt ⚡: Fetch entities and their edge counts in batch to avoid N+1 queries during search ranking.
    pub fn get_entities_with_edge_counts(&self, ids: &[String]) -> Result<Vec<(Entity, usize)>> {
        if ids.is_empty() { return Ok(Vec::new()); }
        let conn = self.conn.lock().unwrap();
        let mut results = Vec::new();
        for chunk in ids.chunks(100) {
            let placeholders: String = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT e.id, e.type, e.name, e.source_page, e.confidence, e.created_at,
                (SELECT COUNT(*) FROM edges WHERE source_id = e.id OR target_id = e.id) as edge_count
                FROM entities e WHERE e.id IN ({})",
                placeholders
            );

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk), |row| {
                let created_at_str: String = row.get(5)?;
                let entity = Entity {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    name: row.get(2)?,
                    source_page: row.get(3)?,
                    confidence: row.get(4)?,
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .unwrap_or_else(|_| chrono::Utc::now().into())
                        .with_timezone(&chrono::Utc),
                };
                let count: usize = row.get(6)?;
                Ok((entity, count))
            })?;

            for row in rows {
                results.push(row?);
            }
        }
        Ok(results)
    }
}
