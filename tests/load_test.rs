//! Test de charge — Stress concurrent de SoulSystem.
//!
//! Lance plusieurs taches concurrentes:
//! - Appels repetes a SoulMemory::store et search
//! - Publications rapides sur le bus
//! - Executions de skills locales
//!
//! Mesure le temps d'execution et verifie l'absence d'erreurs.

use soulsystem::bus::{Bus, Message};
use soulsystem::local_skills::BuiltinSkills;
use soulsystem::soul_memory::SoulMemory;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

#[tokio::test]
async fn concurrent_store_and_search() {
    let mem = Arc::new(SoulMemory::new_test().unwrap());
    let start = Instant::now();

    let mut handles = Vec::new();

    // 10 taches concurrentes de stockage
    for i in 0..10 {
        let mem = mem.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..20 {
                let mut meta = HashMap::new();
                meta.insert("task".into(), i.to_string());
                meta.insert("seq".into(), j.to_string());
                mem.store(&format!("Document de test {} de la tache {}", j, i), meta)
                    .await
                    .unwrap();
            }
        }));
    }

    // 5 taches de recherche
    for _ in 0..5 {
        let mem = mem.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..30 {
                let _ = mem.search("document test", 5).await.unwrap();
            }
        }));
    }

    // Attendre toutes les taches
    for h in handles {
        h.await.unwrap();
    }

    let elapsed = start.elapsed();
    let count = mem.count().unwrap();

    assert!(count >= 100, "expected >= 100 entries, got {}", count);
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "test took too long: {:?}",
        elapsed
    );

    eprintln!(
        "concurrent_store_and_search: {} entries in {:?}",
        count, elapsed
    );
}

#[tokio::test]
async fn bus_publish_stress() {
    let bus = Arc::new(Bus::new(512));
    let start = Instant::now();

    let mut handles = Vec::new();

    // Abonne 5 listeners
    for i in 0..5 {
        let bus = bus.clone();
        handles.push(tokio::spawn(async move {
            let mut rx = bus.subscribe();
            let mut count = 0usize;
            loop {
                match rx.recv().await {
                    Ok(_) => count += 1,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("listener {} lagged by {}", i, n);
                        continue;
                    }
                }
                if count >= 100 {
                    break;
                }
            }
            count
        }));
    }

    // Publie 200 messages rapidement
    for i in 0..200 {
        bus.publish(Message::HnnStatus {
            organ: "perf".to_string(),
            status: "running".to_string(),
            energy: i as f32,
        });
        tokio::task::yield_now().await;
    }

    let mut total_received = 0usize;
    for h in handles {
        total_received += h.await.unwrap();
    }

    let elapsed = start.elapsed();
    eprintln!(
        "bus_publish_stress: {} messages received by 5 listeners in {:?}",
        total_received, elapsed
    );

    assert!(total_received >= 400, "expected >= 400 total receives");
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "bus stress took too long: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn skill_execute_stress() {
    let skills = Arc::new(BuiltinSkills::new());
    let start = Instant::now();

    let mut handles = Vec::new();

    for _ in 0..20 {
        let skills = skills.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..50 {
                let result = skills.execute("echo", &format!("stress test message {}", i));
                assert!(result.is_ok());
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let elapsed = start.elapsed();
    eprintln!("skill_execute_stress: 1000 echo calls in {:?}", elapsed);

    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "skill stress took too long: {:?}",
        elapsed
    );
}
