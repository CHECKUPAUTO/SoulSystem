//! SoulFlow integration test — real end-to-end ingestion.
//!
//! Flow: Read ARCHITECTURE.md → PageProvider → Ollama Embed → Store → Search
//!
//! Run: cargo test -p soullink-rag --test soulflow_integration -- --nocapture

use soullink_rag::{
    embedding::EmbedConfig, provider::PageProvider, store::PageAwareStore, PipelineConfig,
    TextChunker,
};

/// Simple page provider that splits text into character-based chunks.
struct TextPageProvider {
    chunks: Vec<String>,
}

impl TextPageProvider {
    fn from_text(text: &str, chunk_chars: usize) -> Self {
        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        for chunk in chars.chunks(chunk_chars) {
            let s: String = chunk.iter().collect();
            chunks.push(s);
        }
        Self { chunks }
    }
}

impl PageProvider for TextPageProvider {
    fn get_page_count(&self) -> usize {
        self.chunks.len()
    }

    fn get_page_content(&self, idx: usize) -> Option<String> {
        self.chunks.get(idx).cloned()
    }

    fn document(&self) -> soullink_rag::provider::Document {
        soullink_rag::provider::Document {
            id: "arch-md".to_string(),
            title: "SoulLink V13.5 Architecture".to_string(),
            source: "ARCHITECTURE.md".to_string(),
            page_count: 0, // filled later by provider
        }
    }
}

fn read_test_document() -> String {
    let path = "/mnt/nvme/soullink_brain/soullink-v13.5/ARCHITECTURE.md";
    std::fs::read_to_string(path).expect("Failed to read ARCHITECTURE.md")
}

#[tokio::test]
async fn soulflow_full_ingest_and_search() {
    let text = read_test_document();
    println!("📄 Document loaded: {} chars", text.len());

    // 1. Chunk with TextChunker
    let config = PipelineConfig {
        chunk_size: 512,
        chunk_overlap: 64,
        max_chunks: 50,
        ..Default::default()
    };
    let chunker = TextChunker::new(&config);
    let chunks = chunker.chunk(&text);
    println!("✂️  TextChunker: {} chunks", chunks.len());
    assert!(!chunks.is_empty());

    // 2. Ingest via PageAwareStore (uses Ollama embed internally)
    let store_dir = tempfile::tempdir().expect("tempdir");
    let embed_config = EmbedConfig {
        ollama_url: "http://127.0.0.1:11434".to_string(),
        model: "nomic-embed-text".to_string(),
        dimension: 768,
        max_concurrency: 4,
        timeout_secs: 30,
    };

    let store = PageAwareStore::open_default(
        store_dir.path(),
        768,
        soullink_rag::page::PageConfig::default(),
    )
    .expect("store open");

    // Create a provider from the chunks
    let provider = TextPageProvider::from_text(&text, 512);
    println!("📥 Provider: {} pages", provider.get_page_count());

    let start = std::time::Instant::now();
    let count = store.ingest(&provider).await.expect("ingest failed");
    let elapsed = start.elapsed();
    println!("✅ Ingested {} chunks in {:?}", count, elapsed);

    assert!(count > 0, "Should ingest at least some chunks");

    // 3. Search
    let query = "guardian module";
    println!("🔍 Searching: \"{}\"", query);
    let results = store.search(query, 5, 0.7).await.expect("search failed");

    println!("📊 Found {} hits:", results.hits.len());
    for (i, hit) in results.hits.iter().enumerate() {
        println!(
            "  {}. [vec={:.4} kw={:.4}] {}",
            i + 1,
            hit.vector_score,
            hit.keyword_score,
            hit.id
        );
    }

    assert!(
        !results.hits.is_empty(),
        "Should find results for 'guardian module'"
    );
}

#[tokio::test]
async fn soulflow_chunker_quality() {
    let text = read_test_document();

    let config = PipelineConfig {
        chunk_size: 512,
        chunk_overlap: 64,
        max_chunks: 100,
        ..Default::default()
    };
    let chunker = TextChunker::new(&config);
    let chunks = chunker.chunk(&text);

    // Quality checks
    assert!(chunks.len() > 5, "12KB doc should produce >5 chunks");

    // No empty chunks
    for chunk in &chunks {
        assert!(!chunk.trim().is_empty(), "Chunks should not be empty");
    }

    // Chunks should have reasonable sizes
    let avg_len: usize = chunks.iter().map(|c| c.len()).sum::<usize>() / chunks.len().max(1);
    assert!(
        avg_len > 50,
        "Average chunk should be meaningful size, got {}",
        avg_len
    );

    println!(
        "✅ Chunker quality: {} chunks, avg {} chars, min {}, max {}",
        chunks.len(),
        avg_len,
        chunks.iter().map(|c| c.len()).min().unwrap_or(0),
        chunks.iter().map(|c| c.len()).max().unwrap_or(0)
    );
}
