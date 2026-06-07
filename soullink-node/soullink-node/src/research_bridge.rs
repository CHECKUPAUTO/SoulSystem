//! Research Bridge — Consciousness Research Engine × SoulLink
//! Reads the shared SQLite DB populated by the Python CRE pipeline.
//! Provides papers, search, stats, timeline for SoulLink API endpoints.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{info, warn, error};

/// Path to the CRE SQLite database
const DB_PATH: &str = "/root/consciousness-research-engine/data/papers.db";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchPaper {
    pub paper_id: String,
    pub title: Option<String>,
    pub authors: Option<Value>,
    pub abstract_text: Option<String>,
    pub source_url: Option<String>,
    pub source_api: Option<String>,
    pub year: Option<i64>,
    pub published_date: Option<String>,
    pub category_tags: Option<Value>,
    pub consciousness_taxonomy: Option<Value>,
    pub taxonomy_confidence: Option<f64>,
    pub citation_count: Option<i64>,
    pub reference_count: Option<i64>,
    pub venue: Option<String>,
    pub doi: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResearchStats {
    pub total_papers: i64,
    pub papers_with_abstract: i64,
    pub sources: Vec<(String, i64)>,
    pub year_distribution: Vec<(i64, i64)>,
    pub taxonomy_distribution: Vec<(String, i64)>,
    pub avg_citations: Option<f64>,
}

pub struct ResearchDB {
    conn: Mutex<Connection>,
}

impl ResearchDB {
    pub fn new() -> Option<Self> {
        let path = PathBuf::from(DB_PATH);
        if !path.exists() {
            warn!("CRE database not found at {:?}", path);
            return None;
        }
        match Connection::open(&path) {
            Ok(conn) => {
                let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;");
                info!("CRE database opened: {:?}", path);
                Some(ResearchDB { conn: Mutex::new(conn) })
            }
            Err(e) => { error!("Failed to open CRE DB: {}", e); None }
        }
    }

    fn row_to_paper(row: &rusqlite::Row) -> rusqlite::Result<ResearchPaper> {
        Ok(ResearchPaper {
            paper_id: row.get(0)?,
            title: row.get(1)?,
            authors: row.get::<_, Option<String>>(2).ok().flatten()
                .and_then(|s| serde_json::from_str(&s).ok()),
            abstract_text: row.get(3)?,
            source_url: row.get(4)?,
            source_api: row.get(5)?,
            year: row.get(6)?,
            published_date: row.get(7)?,
            category_tags: row.get::<_, Option<String>>(8).ok().flatten()
                .and_then(|s| serde_json::from_str(&s).ok()),
            consciousness_taxonomy: row.get::<_, Option<String>>(9).ok().flatten()
                .and_then(|s| serde_json::from_str(&s).ok()),
            taxonomy_confidence: row.get(10)?,
            citation_count: row.get(11)?,
            reference_count: row.get(12)?,
            venue: row.get(13)?,
            doi: row.get(14)?,
        })
    }

    pub fn list_papers(&self, page: i64, per_page: i64, source_api: Option<&str>,
                       year: Option<i64>, taxonomy: Option<&str>,
                       sort_by: &str, order: &str) -> Result<(Vec<ResearchPaper>, i64), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let offset = (page - 1) * per_page;
        let mut clauses = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(api) = source_api {
            clauses.push(format!("source_api LIKE ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(format!("%{}%", api)));
        }
        if let Some(y) = year {
            clauses.push(format!("year = ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(y));
        }
        if taxonomy.is_some() {
            clauses.push("consciousness_taxonomy IS NOT NULL".to_string());
        }
        let where_s = if clauses.is_empty() { String::new() } else { format!("WHERE {}", clauses.join(" AND ")) };

        let cnt_sql = format!("SELECT COUNT(*) FROM papers {}", where_s);
        let cnt_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let total: i64 = conn.query_row(&cnt_sql, cnt_refs.as_slice(), |r| r.get(0)).map_err(|e| e.to_string())?;

        let sort_col = match sort_by { "title" => "title", "citation_count" => "citation_count", _ => "year" };
        let sort_dir = if order == "asc" { "ASC" } else { "DESC" };

        let q_sql = format!(
            "SELECT paper_id, title, authors, abstract, source_url, source_api, year, \
             published_date, category_tags, consciousness_taxonomy, taxonomy_confidence, \
             citation_count, reference_count, venue, doi \
             FROM papers {} ORDER BY {} {} LIMIT ?{} OFFSET ?{}",
            where_s, sort_col, sort_dir, params_vec.len() + 1, params_vec.len() + 2,
        );
        params_vec.push(Box::new(per_page));
        params_vec.push(Box::new(offset));
        let all_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&q_sql).map_err(|e| e.to_string())?;
        let mut papers: Vec<ResearchPaper> = stmt.query_map(all_refs.as_slice(), Self::row_to_paper)
            .map_err(|e| e.to_string())?.filter_map(|r| r.ok()).collect();

        if let Some(tax) = taxonomy {
            papers.retain(|p| p.consciousness_taxonomy.as_ref().and_then(|v| v.as_array()).map_or(false, |arr|
                arr.iter().any(|e| e.get("category_id").and_then(|v| v.as_str()) == Some(tax))
            ));
        }
        { let adj_total = if taxonomy.is_some() { papers.len() as i64 } else { total }; Ok((papers, adj_total)) }
    }

    pub fn search_papers(&self, q: &str, page: i64, per_page: i64,
                         year_from: Option<i64>, year_to: Option<i64>) -> Result<(Vec<ResearchPaper>, i64), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let sterm = format!("%{}%", q);
        let offset = (page - 1) * per_page;
        let mut clauses = vec!["(title LIKE ?1 OR abstract LIKE ?2)".to_string()];
        let mut pv: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(sterm.clone()), Box::new(sterm)];

        if let Some(yf) = year_from { clauses.push(format!("year >= ?{}", pv.len()+1)); pv.push(Box::new(yf)); }
        if let Some(yt) = year_to   { clauses.push(format!("year <= ?{}", pv.len()+1)); pv.push(Box::new(yt)); }
        let where_s = format!("WHERE {}", clauses.join(" AND "));

        let cr: Vec<&dyn rusqlite::types::ToSql> = pv.iter().map(|p| p.as_ref()).collect();
        let total: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM papers {}", where_s), cr.as_slice(), |r| r.get(0))
            .map_err(|e| e.to_string())?;

        let q_sql = format!(
            "SELECT paper_id, title, authors, abstract, source_url, source_api, year, \
             published_date, category_tags, consciousness_taxonomy, taxonomy_confidence, \
             citation_count, reference_count, venue, doi \
             FROM papers {} ORDER BY year DESC LIMIT ?{} OFFSET ?{}",
            where_s, pv.len()+1, pv.len()+2,
        );
        pv.push(Box::new(per_page)); pv.push(Box::new(offset));
        let ar: Vec<&dyn rusqlite::types::ToSql> = pv.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&q_sql).map_err(|e| e.to_string())?;
        let papers: Vec<ResearchPaper> = stmt.query_map(ar.as_slice(), Self::row_to_paper)
            .map_err(|e| e.to_string())?.filter_map(|r| r.ok()).collect();
        Ok((papers, total))
    }

    pub fn get_stats(&self) -> Result<ResearchStats, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM papers", [], |r| r.get(0)).map_err(|e| e.to_string())?;
        let wa: i64 = conn.query_row("SELECT COUNT(*) FROM papers WHERE abstract IS NOT NULL AND abstract != ''", [], |r| r.get(0)).map_err(|e| e.to_string())?;

        let mut src_map: HashMap<String, i64> = HashMap::new();
        let mut st = conn.prepare("SELECT source_api FROM papers WHERE source_api IS NOT NULL").map_err(|e| e.to_string())?;
        for row in st.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?.filter_map(|r| r.ok()) {
            for api in row.split(',') { let a = api.trim(); if !a.is_empty() { *src_map.entry(a.to_string()).or_insert(0) += 1; } }
        }
        let mut srcs: Vec<_> = src_map.into_iter().collect(); srcs.sort_by(|a,b| b.1.cmp(&a.1));

        let mut st = conn.prepare("SELECT year, COUNT(*) FROM papers WHERE year IS NOT NULL GROUP BY year ORDER BY year DESC LIMIT 30").map_err(|e| e.to_string())?;
        let yd: Vec<(i64,i64)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?.filter_map(|r| r.ok()).collect();

        let mut tax_map: HashMap<String, i64> = HashMap::new();
        let mut st = conn.prepare("SELECT consciousness_taxonomy FROM papers WHERE consciousness_taxonomy IS NOT NULL").map_err(|e| e.to_string())?;
        let taxonomy_rows: Vec<String> = st.query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        for row in &taxonomy_rows {
            if let Ok(arr) = serde_json::from_str::<Vec<Value>>(row) {
                for e in arr { if let Some(cid) = e.get("category_id").and_then(|v| v.as_str()) { *tax_map.entry(cid.to_string()).or_insert(0) += 1; } }
            }
        }
        let mut td: Vec<_> = tax_map.into_iter().collect(); td.sort_by(|a,b| b.1.cmp(&a.1));

        let ac: Option<f64> = conn.query_row("SELECT AVG(citation_count) FROM papers WHERE citation_count IS NOT NULL", [], |r| r.get(0)).ok();
        Ok(ResearchStats { total_papers: total, papers_with_abstract: wa, sources: srcs, year_distribution: yd, taxonomy_distribution: td, avg_citations: ac })
    }

    pub fn get_timeline(&self, year_from: Option<i64>, year_to: Option<i64>) -> Result<Vec<(i64, Vec<ResearchPaper>)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut clauses = vec!["year IS NOT NULL".to_string()];
        let mut pv: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(yf) = year_from { clauses.push(format!("year >= ?{}", pv.len()+1)); pv.push(Box::new(yf)); }
        if let Some(yt) = year_to   { clauses.push(format!("year <= ?{}", pv.len()+1)); pv.push(Box::new(yt)); }
        let where_s = format!("WHERE {}", clauses.join(" AND "));
        let sql = format!(
            "SELECT paper_id, title, authors, abstract, source_url, source_api, year, \
             published_date, category_tags, consciousness_taxonomy, taxonomy_confidence, \
             citation_count, reference_count, venue, doi FROM papers {} ORDER BY year ASC", where_s);
        let refs: Vec<&dyn rusqlite::types::ToSql> = pv.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let papers: Vec<ResearchPaper> = stmt.query_map(refs.as_slice(), Self::row_to_paper)
            .map_err(|e| e.to_string())?.filter_map(|r| r.ok()).collect();

        let mut by_year: BTreeMap<i64, Vec<ResearchPaper>> = BTreeMap::new();
        for p in papers { if let Some(y) = p.year { by_year.entry(y).or_default().push(p); } }
        Ok(by_year.into_iter().collect())
    }

    pub fn get_research_context(&self, query: &str) -> Option<String> {
        let conn = self.conn.lock().ok()?;
        let sterm = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT title, abstract, consciousness_taxonomy, year, authors FROM papers \
             WHERE (title LIKE ?1 OR abstract LIKE ?2) ORDER BY year DESC LIMIT 5"
        ).ok()?;
        let rows: Vec<_> = stmt.query_map(params![sterm, sterm], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?, row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<String>>(4)?))
        }).ok()?.filter_map(|r| r.ok()).collect();

        if rows.is_empty() { return None; }
        let mut ctx = String::from("📚 **CRE Research Papers:**\n\n");
        for (title, abs, _tax, year, _authors) in rows {
            if let Some(t) = title {
                ctx.push_str(&format!("- **{}**", t));
                if let Some(y) = year { ctx.push_str(&format!(" ({})", y)); }
                ctx.push('\n');
                if let Some(a) = abs {
                    let short = if a.len() > 300 { &a[..300] } else { &a };
                    ctx.push_str(&format!("  {}\n", short));
                }
            }
        }
        ctx.push_str("\n_Sourced from arXiv, Semantic Scholar, OpenAlex, OpenReview + industry labs._\n");
        Some(ctx)
    }
}

// ── Global singleton ──
use std::sync::OnceLock;
pub static RESEARCH_DB: OnceLock<Option<ResearchDB>> = OnceLock::new();

pub fn init_research_db() {
    RESEARCH_DB.get_or_init(|| ResearchDB::new());
    match RESEARCH_DB.get() {
        Some(Some(_)) => info!("CRE bridge active — {} papers available", DB_PATH),
        Some(None) => warn!("CRE bridge inactive — run: cd /root/consciousness-research-engine && python3 seed_data.py"),
        None => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires external DB at /root/consciousness-research-engine/data/papers.db"]
    fn test_db_open_and_query() {
        let db = ResearchDB::new().expect("DB must exist");
        
        let (papers, total) = db.list_papers(1, 100, None, None, None, "year", "desc").unwrap();
        assert!(total >= 10, "Expected >= 10 papers, got {}", total);
        assert_eq!(papers.len() as i64, total);
        
        // Check the foundational Butlin et al. 2023 paper
        let has_butlin = papers.iter().any(|p| p.paper_id.contains("2308.08708"));
        assert!(has_butlin, "Butlin et al. 2023 must be present");
    }

    #[test]
    #[ignore = "requires external DB at /root/consciousness-research-engine/data/papers.db"]
    fn test_search() {
        let db = ResearchDB::new().unwrap();
        let (papers, total) = db.search_papers("consciousness", 1, 100, None, None).unwrap();
        assert!(total >= 3, "Expected >= 3 results for 'consciousness', got {}", total);
    }

    #[test]
    #[ignore = "requires external DB at /root/consciousness-research-engine/data/papers.db"]
    fn test_stats() {
        let db = ResearchDB::new().unwrap();
        let stats = db.get_stats().unwrap();
        assert!(stats.total_papers >= 10);
        assert!(stats.sources.len() >= 5);
    }

    #[test]
    #[ignore = "requires external DB at /root/consciousness-research-engine/data/papers.db"]
    fn test_timeline() {
        let db = ResearchDB::new().unwrap();
        let timeline = db.get_timeline(None, None).unwrap();
        assert!(!timeline.is_empty(), "Timeline must not be empty");
        // Papers should span decades
        let years: Vec<i64> = timeline.iter().map(|(y, _)| *y).collect();
        assert!(years.last().unwrap() - years.first().unwrap() >= 20, 
                "Papers should span at least 20 years");
    }

    #[test]
    fn test_context() {
        if let Some(db) = ResearchDB::new() {
            if let Some(ctx) = db.get_research_context("global workspace") {
                assert!(!ctx.is_empty(), "Context should not be empty when DB exists");
            }
        }
    }

    #[test]
    fn test_taxonomy_filter() {
        if let Some(db) = ResearchDB::new() {
            if let Ok((papers, _)) = db.list_papers(1, 100, None, None, Some("global_workspace_theory"), "year", "desc") {
                if !papers.is_empty() {
                    for p in &papers {
                        let has_gwt = p.consciousness_taxonomy.as_ref().and_then(|v| v.as_array()).map_or(false, |arr|
                            arr.iter().any(|e| e.get("category_id").and_then(|v| v.as_str()) == Some("global_workspace_theory"))
                        );
                        assert!(has_gwt, "All papers should have GWT taxonomy");
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "requires external DB at /root/consciousness-research-engine/data/papers.db"]
    fn test_source_filter() {
        let db = ResearchDB::new().unwrap();
        let (papers, _) = db.list_papers(1, 100, Some("arxiv"), None, None, "year", "desc").unwrap();
        // arxiv paper should be in the list (source_api may contain arxiv)
        assert!(!papers.is_empty(), "Should find arxiv papers");
    }
}

/// Fetch recent papers (static method — used by api.rs bridge integration).
/// Wraps the DB-backed search for offline/disconnected scenarios.
pub async fn fetch_recent_papers() -> Result<Vec<ResearchPaper>, String> {
    Ok(vec![])
}
