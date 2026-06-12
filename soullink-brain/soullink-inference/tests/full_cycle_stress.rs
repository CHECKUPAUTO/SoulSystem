//! Full cycle stress test — validates VRAM-aware routing end-to-end.
//! Simulates: LLM request → VramWait → Ollama purge → CUTLASS search.
//!
//! Run: cargo test -p soullink-inference --test full_cycle_stress -- --nocapture

use soullink_inference::router::ModelRouter;
use soullink_inference::types::*;

/// Simulate the full VRAM lifecycle:
/// 1. LLM loaded (VRAM low) → VectorSearch returns VramWait
/// 2. Ollama purged (VRAM freed) → VectorSearch returns CutlassSearch
/// 3. HNN always works (tiny footprint)
#[test]
fn full_cycle_vram_lifecycle() {
    let router = ModelRouter::new();

    // ── Phase 1: Ollama loaded, VRAM tight ──────────────────────────────
    let snap_busy = HardwareSnapshot {
        numa_nodes: vec![
            NumaNodeStatus {
                node_id: 0,
                cpu_cores: vec![],
                memory_total_gb: 64.0,
                memory_free_gb: 30.0,
                l3_cache_kb: 30720,
            },
            NumaNodeStatus {
                node_id: 1,
                cpu_cores: vec![],
                memory_total_gb: 64.0,
                memory_free_gb: 28.0,
                l3_cache_kb: 30720,
            },
        ],
        gpu: GpuStatus {
            vram_total_gb: 8.0,
            vram_free_gb: 0.5, // Ollama loaded: only 500MB free
            gpu_temp_c: 72,
            sm_clock_mhz: 2100,
            max_sm_clock_mhz: 2790,
            throttle_active: false,
        },
        timestamp_ms: 0,
    };

    // VectorSearch should be VramWait
    let search_req = GenerateRequest {
        model: "cutlass".into(),
        prompt: "search".into(),
        max_tokens: 0,
        temperature: 0.0,
        priority: Priority::VectorSearch,
        quantization: None,
    };
    match router.route(&search_req, &snap_busy) {
        ExecutionTarget::VramWait => {
            println!("✅ Phase 1: VectorSearch → VramWait (Ollama loaded, 0.5GB free)")
        }
        other => panic!("Expected VramWait, got {:?}", other),
    }

    // Preprocess should also be VramWait
    let preprocess_req = GenerateRequest {
        model: "dali".into(),
        prompt: "preprocess".into(),
        max_tokens: 0,
        temperature: 0.0,
        priority: Priority::Preprocess,
        quantization: None,
    };
    match router.route(&preprocess_req, &snap_busy) {
        ExecutionTarget::VramWait => {
            println!("✅ Phase 1b: Preprocess → VramWait (0.5GB < 2GB threshold)")
        }
        other => panic!("Expected VramWait, got {:?}", other),
    }

    // HNN should still work (tiny)
    let hnn_req = GenerateRequest {
        model: "hnn".into(),
        prompt: "step".into(),
        max_tokens: 0,
        temperature: 0.0,
        priority: Priority::HnnStep,
        quantization: None,
    };
    match router.route(&hnn_req, &snap_busy) {
        ExecutionTarget::CutlassHnn => {
            println!("✅ Phase 1c: HnnStep → CutlassHnn (fits in 500MB)")
        }
        other => panic!("Expected CutlassHnn, got {:?}", other),
    }

    // Think should fall back to CPU
    let think_req = GenerateRequest {
        model: "qwen2.5-coder:3b".into(),
        prompt: "think".into(),
        max_tokens: 100,
        temperature: 0.7,
        priority: Priority::Think,
        quantization: None,
    };
    match router.route(&think_req, &snap_busy) {
        ExecutionTarget::CpuNuma { node, .. } => println!(
            "✅ Phase 1d: Think → CpuNuma-{} (VRAM too low for LLM)",
            node
        ),
        ExecutionTarget::FallbackOllama => {
            println!("✅ Phase 1d: Think → FallbackOllama (acceptable)")
        }
        other => panic!("Expected CpuNuma or Ollama, got {:?}", other),
    }

    // ── Phase 2: Ollama purged (5 min idle), VRAM freed ────────────────
    let snap_free = HardwareSnapshot {
        numa_nodes: snap_busy.numa_nodes.clone(),
        gpu: GpuStatus {
            vram_total_gb: 8.0,
            vram_free_gb: 7.5, // Ollama purged: 7.5GB free
            gpu_temp_c: 45,
            sm_clock_mhz: 2100,
            max_sm_clock_mhz: 2790,
            throttle_active: false,
        },
        timestamp_ms: 0,
    };

    // VectorSearch should now go to CUTLASS
    match router.route(&search_req, &snap_free) {
        ExecutionTarget::CutlassSearch => {
            println!("✅ Phase 2: VectorSearch → CutlassSearch (7.5GB free)")
        }
        other => panic!("Expected CutlassSearch, got {:?}", other),
    }

    // Preprocess should now go to DALI
    match router.route(&preprocess_req, &snap_free) {
        ExecutionTarget::DaliGpu => println!("✅ Phase 2b: Preprocess → DaliGpu (7.5GB > 2GB)"),
        other => panic!("Expected DaliGpu, got {:?}", other),
    }

    // Think should now go to GPU
    match router.route(&think_req, &snap_free) {
        ExecutionTarget::Gpu(_) => println!("✅ Phase 2c: Think → Gpu (VRAM available)"),
        other => panic!("Expected Gpu, got {:?}", other),
    }

    println!("\n🟢 Full cycle stress test PASSED — VRAM-aware routing is operational.");
}

/// Test the transition boundary: exactly at the threshold.
#[test]
fn vram_threshold_boundary() {
    let router = ModelRouter::new();
    let search_req = GenerateRequest {
        model: "cutlass".into(),
        prompt: "search".into(),
        max_tokens: 0,
        temperature: 0.0,
        priority: Priority::VectorSearch,
        quantization: None,
    };

    // Just below threshold (0.9GB < 1.0GB)
    let snap_below = HardwareSnapshot {
        numa_nodes: vec![NumaNodeStatus {
            node_id: 0,
            cpu_cores: vec![],
            memory_total_gb: 64.0,
            memory_free_gb: 30.0,
            l3_cache_kb: 30720,
        }],
        gpu: GpuStatus {
            vram_total_gb: 8.0,
            vram_free_gb: 0.9,
            gpu_temp_c: 70,
            sm_clock_mhz: 2100,
            max_sm_clock_mhz: 2790,
            throttle_active: false,
        },
        timestamp_ms: 0,
    };
    assert!(matches!(
        router.route(&search_req, &snap_below),
        ExecutionTarget::VramWait
    ));

    // Just above threshold (1.1GB > 1.0GB)
    let snap_above = HardwareSnapshot {
        numa_nodes: snap_below.numa_nodes.clone(),
        gpu: GpuStatus {
            vram_total_gb: 8.0,
            vram_free_gb: 1.1,
            gpu_temp_c: 70,
            sm_clock_mhz: 2100,
            max_sm_clock_mhz: 2790,
            throttle_active: false,
        },
        timestamp_ms: 0,
    };
    assert!(matches!(
        router.route(&search_req, &snap_above),
        ExecutionTarget::CutlassSearch
    ));

    println!("✅ Threshold boundary: 0.9GB→VramWait, 1.1GB→CutlassSearch");
}
