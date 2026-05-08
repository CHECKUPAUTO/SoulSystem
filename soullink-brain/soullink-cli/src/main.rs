//! soullink-cli — Command-line interface for SoulFlow pipeline.
//!
//! Commands:
//!   ingest <path>   — Ingest a document into the RAG store
//!   search <query>  — Search the knowledge base
//!   status          — Show RAG store status

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "soullink", version, about = "SoulLink V13.5 — Knowledge Pipeline CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ingest a document (txt, md) into the RAG store
    Ingest {
        /// Path to the document
        path: PathBuf,
        /// Document title (default: filename)
        #[arg(short, long)]
        title: Option<String>,
        /// Chunk size in characters
        #[arg(short, long, default_value = "512")]
        chunk_size: usize,
        /// Chunk overlap
        #[arg(long, default_value = "64")]
        overlap: usize,
    },
    /// Search the knowledge base
    Search {
        /// Search query
        query: String,
        /// Number of results
        #[arg(short = 'k', long, default_value = "5")]
        top_k: usize,
        /// Vector/keyword fusion weight (0=keyword only, 1=vector only)
        #[arg(short, long, default_value = "0.7")]
        alpha: f64,
    },
    /// Show RAG store status
    Status,
    /// Monitor the Neural Mesh (Orchestrator + Organs)
    Mesh {
        /// Orchestrator URL
        #[arg(short, long, default_value = "http://127.0.0.1:9020")]
        url: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Ingest { path, title, chunk_size, overlap } => {
            ingest_cmd(path, title, chunk_size, overlap).await
        }
        Commands::Search { query, top_k, alpha } => {
            search_cmd(query, top_k, alpha).await
        }
        Commands::Status => status_cmd(),
        Commands::Mesh { url } => mesh_cmd(&url).await,
    }
}

async fn ingest_cmd(path: PathBuf, title: Option<String>, chunk_size: usize, overlap: usize) -> Result<()> {
    let text = std::fs::read_to_string(&path)
        .context(format!("Failed to read {:?}", path))?;
    let title = title.unwrap_or_else(|| {
        path.file_name().unwrap_or_default().to_string_lossy().to_string()
    });

    println!("📄 Loading: {} ({} chars)", path.display(), text.len());

    // Chunk
    let config = soullink_rag::PipelineConfig {
        chunk_size,
        chunk_overlap: overlap,
        ..Default::default()
    };
    let chunker = soullink_rag::TextChunker::new(&config);
    let chunks = chunker.chunk(&text);
    println!("✂️  Chunked: {} chunks (size={}, overlap={})", chunks.len(), chunk_size, overlap);

    // Ingest via PageAwareStore
    let store_path = std::path::Path::new("/var/lib/soullink/rag-store");
    std::fs::create_dir_all(store_path)?;

    let store = soullink_rag::store::PageAwareStore::open_default(
        store_path,
        config.embed_dim,
        soullink_rag::page::PageConfig::default(),
    ).context("Failed to open RAG store")?;

    let provider = CliPageProvider { chunks, title: title.clone(), source: path.display().to_string() };
    
    let start = std::time::Instant::now();
    let count = store.ingest(&provider).await.context("Ingestion failed")?;
    let elapsed = start.elapsed();

    println!("✅ Ingested {} chunks in {:?}", count, elapsed);
    println!("   Document: {}", title);
    
    Ok(())
}

async fn mesh_cmd(url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    println!("🕸️  Connecting to Mesh Orchestrator: {}", url);

    let resp = client.get(format!("{}/api/mesh/status", url))
        .send().await.context("Connecting to orchestrator")?;

    if !resp.status().is_success() {
        anyhow::bail!("Orchestrator error: {}", resp.status());
    }

    let data: serde_json::Value = resp.json().await?;
    let mesh = data.get("mesh").and_then(|m| m.as_object());

    println!("---------------------------------------------------------------");
    println!("{:<15} | {:<10} | {:<8} | {:<15}", "ORGAN", "STATE", "N", "ATTRACTOR");
    println!("---------------------------------------------------------------");

    if let Some(mesh) = mesh {
        for (name, state) in mesh {
            let status = state.get("state").and_then(|v| v.as_str()).unwrap_or("?");
            let n = state.get("N").and_then(|v| v.as_u64()).unwrap_or(0);
            let attractor = state.get("attractor").and_then(|v| v.as_str()).unwrap_or("?");

            let color_status = if status == "online" { "✅ online" } else { "❌ offline" };

            println!("{:<15} | {:<10} | {:<8} | {:<15}", name, color_status, n, attractor);
        }
    }

    let total_n = data.get("total_N").and_then(|v| v.as_u64()).unwrap_or(0);
    let online = data.get("online").and_then(|v| v.as_u64()).unwrap_or(0);

    println!("---------------------------------------------------------------");
    println!("Summary: {}/{} organs online | Total N: {}", online, data.get("total_brains").unwrap_or(&serde_json::json!(0)), total_n);

    Ok(())
}

async fn search_cmd(query: String, top_k: usize, alpha: f64) -> Result<()> {
    let store_path = std::path::Path::new("/var/lib/soullink/rag-store");
    
    if !store_path.exists() {
        anyhow::bail!("RAG store not found at {:?}. Run 'soullink ingest' first.", store_path);
    }

    let store = soullink_rag::store::PageAwareStore::open_default(
        store_path,
        768,
        soullink_rag::page::PageConfig::default(),
    ).context("Failed to open RAG store")?;

    println!("🔍 Searching: \"{}\" (top-{}, α={})", query, top_k, alpha);

    let start = std::time::Instant::now();
    let results = store.search(&query, top_k, alpha).await.context("Search failed")?;
    let elapsed = start.elapsed();

    if results.hits.is_empty() {
        println!("❌ No results found.");
        return Ok(());
    }

    println!("📊 {} hits in {:?}:", results.hits.len(), elapsed);
    for (i, hit) in results.hits.iter().enumerate() {
        println!(
            "  {}. [vec={:.3} kw={:.3}] {} ({})",
            i + 1, hit.vector_score, hit.keyword_score, hit.id, hit.doc_id
        );
    }

    Ok(())
}

fn status_cmd() -> Result<()> {
    let store_path = std::path::Path::new("/var/lib/soullink/rag-store");
    
    println!("🧠 SoulLink RAG Store Status");
    println!("   Path: {:?}", store_path);
    println!("   Exists: {}", store_path.exists());

    if store_path.exists() {
        let size = fs_extra::dir::get_size(store_path).unwrap_or(0);
        println!("   Size: {:.2} MB", size as f64 / 1e6);
    }

    // Check Ollama
    println!("\n🦙 Ollama");
    match reqwest::blocking::get("http://127.0.0.1:11434/api/ps") {
        Ok(resp) => {
            if let Ok(data) = resp.json::<serde_json::Value>() {
                let models = data.get("models").and_then(|m| m.as_array());
                match models {
                    Some(m) if !m.is_empty() => {
                        for model in m {
                            let name = model.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                            let size = model.get("size").and_then(|s| s.as_f64()).unwrap_or(0.0);
                            println!("   Loaded: {} ({:.1}GB)", name, size / 1e9);
                        }
                    }
                    _ => println!("   No models loaded"),
                }
            }
        }
        Err(_) => println!("   ❌ Not reachable"),
    }

    Ok(())
}

// ── PageProvider for CLI ─────────────────────────────────────────────────────

struct CliPageProvider {
    chunks: Vec<String>,
    title: String,
    source: String,
}

impl soullink_rag::provider::PageProvider for CliPageProvider {
    fn get_page_count(&self) -> usize {
        self.chunks.len()
    }

    fn get_page_content(&self, idx: usize) -> Option<String> {
        self.chunks.get(idx).cloned()
    }

    fn document(&self) -> soullink_rag::provider::Document {
        soullink_rag::provider::Document {
            id: format!("cli-{}", uuid::Uuid::new_v4()),
            title: self.title.clone(),
            source: self.source.clone(),
            page_count: self.chunks.len(),
        }
    }
}