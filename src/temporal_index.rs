//! TemporalIndex — Indexation temporelle des souvenirs.
//!
//! # Problème
//! Aucune requête chronologique possible. Les souvenirs sont stockés sans
//! index temporel, rendant impossible les requêtes du type "donne-moi les
//! souvenirs d'hier entre 14h et 16h".
//!
//! # Solution
//! Une base SQLite légère avec table `events` qui horodate chaque ajout.
//! Expose une API de requête par plage de temps avec pagination.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

// ── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeIndexEntry {
    pub id: String,
    pub timestamp_ms: i64,
    pub tag: String,
    pub summary: String,
    pub metadata: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRangeQuery {
    pub start_ms: i64,
    pub end_ms: i64,
    pub tag_filter: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

// ── Index temporel ─────────────────────────────────────────────────

/// Index temporel basé sur SQLite pour les souvenirs.
pub struct TemporalIndex {
    db: rusqlite::Connection,
}

impl TemporalIndex {
    /// Ouvre ou crée l'index temporel.
    pub fn open(data_dir: &PathBuf) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("temporal_index.db");

        let db = rusqlite::Connection::open(&db_path)?;

        // Activer WAL pour les performances
        db.execute_batch("PRAGMA journal_mode=WAL;")?;
        db.execute_batch("PRAGMA synchronous=NORMAL;")?;

        // Table des événements
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                timestamp_ms INTEGER NOT NULL,
                tag TEXT NOT NULL DEFAULT '',
                summary TEXT NOT NULL DEFAULT '',
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp_ms);
            CREATE INDEX IF NOT EXISTS idx_events_tag ON events(tag);
            CREATE INDEX IF NOT EXISTS idx_events_timestamp_tag ON events(timestamp_ms, tag);"
        )?;

        info!("TemporalIndex: ouvert à {}", db_path.display());
        Ok(Self { db })
    }

    /// Insère une entrée dans l'index temporel.
    pub fn insert(&self, entry: &TimeIndexEntry) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        self.db.execute(
            "INSERT OR REPLACE INTO events (id, timestamp_ms, tag, summary, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                entry.id,
                entry.timestamp_ms,
                entry.tag,
                entry.summary,
                entry.metadata,
                now,
            ],
        )?;
        Ok(())
    }

    /// Insère une entrée simple (version courte).
    pub fn insert_simple(&self, id: &str, tag: &str, summary: &str) -> Result<()> {
        let entry = TimeIndexEntry {
            id: id.to_string(),
            timestamp_ms: Utc::now().timestamp_millis(),
            tag: tag.to_string(),
            summary: summary.to_string(),
            metadata: "{}".to_string(),
        };
        self.insert(&entry)
    }

    /// Requête chronologique.
    pub fn query_range(&self, query: &TimeRangeQuery) -> Result<Vec<TimeIndexEntry>> {
        let mut sql = String::from(
            "SELECT id, timestamp_ms, tag, summary, metadata FROM events
             WHERE timestamp_ms >= ?1 AND timestamp_ms <= ?2"
        );

        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
            Box::new(query.start_ms),
            Box::new(query.end_ms),
        ];

        if let Some(ref tag) = query.tag_filter {
            sql.push_str(" AND tag = ?3");
            params.push(Box::new(tag.clone()));
        }

        sql.push_str(" ORDER BY timestamp_ms DESC");
        sql.push_str(&format!(" LIMIT {} OFFSET {}", query.limit, query.offset));

        let mut stmt = self.db.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let entries = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(TimeIndexEntry {
                id: row.get(0)?,
                timestamp_ms: row.get(1)?,
                tag: row.get(2)?,
                summary: row.get(3)?,
                metadata: row.get(4)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

        Ok(entries)
    }

    /// Requête sur une période relative (ex: dernières 24h).
    pub fn query_recent(&self, hours: i64, tag_filter: Option<&str>, limit: usize) -> Result<Vec<TimeIndexEntry>> {
        let now = Utc::now().timestamp_millis();
        let start = now - hours * 3600 * 1000;
        self.query_range(&TimeRangeQuery {
            start_ms: start,
            end_ms: now,
            tag_filter: tag_filter.map(|s| s.to_string()),
            limit,
            offset: 0,
        })
    }

    /// Requête sur une date spécifique (YYYY-MM-DD).
    pub fn query_date(&self, date: &str, tag_filter: Option<&str>, limit: usize) -> Result<Vec<TimeIndexEntry>> {
        // Convertir la date en timestamps
        let start = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap()
                .and_utc()
                .timestamp_millis())
            .unwrap_or(0);
        let end = start + 24 * 3600 * 1000;

        self.query_range(&TimeRangeQuery {
            start_ms: start,
            end_ms: end,
            tag_filter: tag_filter.map(|s| s.to_string()),
            limit,
            offset: 0,
        })
    }

    /// Compte le nombre d'entrées.
    pub fn count(&self) -> Result<usize> {
        let count: usize = self.db.query_row(
            "SELECT COUNT(*) FROM events",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Supprime les entrées plus vieilles qu'un certain nombre de jours.
    pub fn prune_older_than(&self, days: i64) -> Result<usize> {
        let cutoff = Utc::now().timestamp_millis() - days * 24 * 3600 * 1000;
        let deleted = self.db.execute(
            "DELETE FROM events WHERE timestamp_ms < ?1",
            rusqlite::params![cutoff],
        )?;
        info!("TemporalIndex: {} entrées purgées (> {} jours)", deleted, days);
        Ok(deleted)
    }

    /// Récupère les tags distincts.
    pub fn distinct_tags(&self) -> Result<Vec<String>> {
        let mut stmt = self.db.prepare("SELECT DISTINCT tag FROM events ORDER BY tag")?;
        let tags = stmt.query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_insert_and_query() {
        let dir = TempDir::new().unwrap();
        let index = TemporalIndex::open(&dir.path().to_path_buf()).unwrap();

        index.insert_simple("mem1", "test", "Premier souvenir").unwrap();
        index.insert_simple("mem2", "test", "Deuxième souvenir").unwrap();

        assert_eq!(index.count().unwrap(), 2);
    }

    #[test]
    fn test_time_range_query() {
        let dir = TempDir::new().unwrap();
        let index = TemporalIndex::open(&dir.path().to_path_buf()).unwrap();

        let now = Utc::now().timestamp_millis();
        index.insert(&TimeIndexEntry {
            id: "old".into(),
            timestamp_ms: now - 3600_000, // 1h ago
            tag: "historique".into(),
            summary: "Vieux souvenir".into(),
            metadata: "{}".into(),
        }).unwrap();

        index.insert(&TimeIndexEntry {
            id: "new".into(),
            timestamp_ms: now,
            tag: "recent".into(),
            summary: "Souvenir récent".into(),
            metadata: "{}".into(),
        }).unwrap();

        // Requête sur la dernière heure
        let results = index.query_recent(1, None, 10).unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|e| e.id == "new"));
    }

    #[test]
    fn test_tag_filter() {
        let dir = TempDir::new().unwrap();
        let index = TemporalIndex::open(&dir.path().to_path_buf()).unwrap();

        index.insert_simple("a", "important", "Fait important").unwrap();
        index.insert_simple("b", "normal", "Fait normal").unwrap();

        let results = index.query_recent(24, Some("important"), 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a");
    }

    #[test]
    fn test_distinct_tags() {
        let dir = TempDir::new().unwrap();
        let index = TemporalIndex::open(&dir.path().to_path_buf()).unwrap();

        index.insert_simple("a", "alpha", "Test A").unwrap();
        index.insert_simple("b", "beta", "Test B").unwrap();
        index.insert_simple("c", "alpha", "Test C").unwrap();

        let tags = index.distinct_tags().unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"alpha".to_string()));
        assert!(tags.contains(&"beta".to_string()));
    }

    #[test]
    fn test_prune() {
        let dir = TempDir::new().unwrap();
        let index = TemporalIndex::open(&dir.path().to_path_buf()).unwrap();

        let old_ts = Utc::now().timestamp_millis() - 10 * 24 * 3600_000; // 10 days ago
        index.insert(&TimeIndexEntry {
            id: "old".into(),
            timestamp_ms: old_ts,
            tag: "old".into(),
            summary: "Old".into(),
            metadata: "{}".into(),
        }).unwrap();

        index.insert_simple("new", "new", "Recent").unwrap();

        let pruned = index.prune_older_than(7).unwrap();
        assert_eq!(pruned, 1);
        assert_eq!(index.count().unwrap(), 1);
    }
}
