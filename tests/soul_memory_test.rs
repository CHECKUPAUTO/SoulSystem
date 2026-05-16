use soulsystem::soul_memory::SoulMemory;
use std::collections::HashMap;

#[tokio::test]
async fn test_store_and_search() {
    let mem = SoulMemory::new_test().expect("Failed to create test memory");

    let mut meta1 = HashMap::new();
    meta1.insert("source".into(), "arxiv".into());

    let mut meta2 = HashMap::new();
    meta2.insert("source".into(), "web".into());

    mem.store("Quantum computing with HNN meshes", meta1)
        .await
        .unwrap();
    mem.store("Classical machine learning techniques", meta2)
        .await
        .unwrap();

    let results = mem.search("quantum", 3).await.unwrap();
    assert!(!results.is_empty(), "Should find at least one result");
}

#[tokio::test]
async fn test_get_context() {
    let mem = SoulMemory::new_test().expect("Failed to create test memory");

    let mut meta = HashMap::new();
    meta.insert("source".into(), "test".into());

    mem.store("Neural networks and backpropagation", meta.clone())
        .await
        .unwrap();
    mem.store("Deep learning with transformers", meta.clone())
        .await
        .unwrap();

    let ctx = mem.get_context("neural networks").await.unwrap();
    assert!(!ctx.is_empty(), "Context should not be empty");
    assert!(ctx.contains("Neural networks") || ctx.contains("Deep learning"));
}

#[tokio::test]
async fn test_empty_search() {
    let mem = SoulMemory::new_test().expect("Failed to create test memory");
    let results = mem.search("nonexistent", 5).await.unwrap();
    assert!(results.is_empty(), "Should return empty for no matches");
}
