/// Mamba SSM — extended demonstration example.
///
/// Builds a small Mamba model and demonstrates:
///   1. Forward pass + greedy generation
///   2. INT8 quantization of weights
///   3. Cached autoregressive generation
///   4. Tiled (IO-aware) scan
///   5. Grouped gating (GQA-style)
///   6. CUDA-style parallel scan (simulated)
///   7. Weight tying
///
/// Usage:
///   cargo run --example mamba_demo
use ndarray::Array1;
use soullink_ssm::cuda::CudaScan;
use soullink_ssm::layer::GateType;
use soullink_ssm::model::{MambaModel, MambaModelConfig};
use soullink_ssm::parallel::ScanElement;
use soullink_ssm::quantization::{quantization_error, QuantizedEmbedding, QuantizedMatrix};
use soullink_ssm::tiled_scan::TiledScan;

fn main() {
    println!("═══ Soullink SSM — Extended Demo ═══\n");

    // ── 1. Standard Mamba forward ──────────────────────────────────────
    println!("─── 1. Standard Mamba Model ───");
    let config = MambaModelConfig {
        vocab_size: 100,
        n_layers: 2,
        d_model: 32,
        state_dim: 8,
        expand_factor: 2,
        conv_kernel: 4,
        eps: 1e-5,
        weight_tied: false,
        gate_type: GateType::SiLU,
    };

    let model = MambaModel::new(config);
    println!("  vocab_size: {}", model.config.vocab_size);
    println!("  layers:     {}", model.config.n_layers);
    println!("  d_model:    {}", model.config.d_model);
    println!("  state_dim:  {}", model.config.state_dim);
    println!("  params:     ~{}\n", model.param_count());

    let input_ids: Vec<usize> = vec![5, 12, 33, 47, 8, 19, 72, 91];
    let logits = model.forward(&input_ids);
    println!("  Forward: [{}, {}]", logits.shape()[0], logits.shape()[1]);
    let next = model.generate_next(&input_ids);
    println!("  Greedy next token: {}\n", next);

    // ── 2. INT8 Quantization ───────────────────────────────────────────
    println!("─── 2. INT8 Quantization ───");
    let q_embed = QuantizedEmbedding::quantize(&model.embedding);
    let err = quantization_error(&model.embedding, &q_embed.inner.dequantize());
    println!(
        "  Embedding:  {} tokens × {} dims",
        q_embed.vocab_size, q_embed.d_model
    );
    println!(
        "  Memory ratio: {:.1}% of f64",
        q_embed.inner.memory_ratio() * 100.0
    );
    println!("  SNR: {:.1} dB", err.snr_db);
    println!("  Max error: {:.6}\n", err.max_abs_error);

    // Quantize the first layer's projection weights
    if let Some(layer) = model.layers.first() {
        let q_in_proj = QuantizedMatrix::quantize(&layer.in_proj);
        let dq = q_in_proj.dequantize();
        let perr = quantization_error(&layer.in_proj, &dq);
        println!(
            "  InProj quantization: SNR {:.1} dB, max err {:.6}",
            perr.snr_db, perr.max_abs_error
        );
        println!(
            "  Memory: {:.1}% of f64\n",
            q_in_proj.memory_ratio() * 100.0
        );
    }

    // ── 3. Cached autoregressive generation ───────────────────────────
    println!("─── 3. Cached Inference ───");
    let mut cache = model.new_cache();
    let (next_token, _) = model.generate_cached(&input_ids, &mut cache);
    println!("  Warmup complete. Next token: {}", next_token);

    // Single-token generation steps using cache
    println!("  Generating 5 tokens with cache:");
    let mut current = next_token;
    for step in 0..5 {
        let (token, _) = model.generate_cached(&[current], &mut cache);
        println!("    step {}: token {}", step + 1, token);
        current = token;
    }
    println!();

    // ── 4. Tiled scan (IO-aware) ───────────────────────────────────────
    println!("─── 4. Tiled Scan ───");
    use soullink_ssm::ssm;
    let n = 4;
    let a = ndarray::Array2::eye(n) * -0.5;
    let b_shared: Vec<Array1<f64>> = (0..16)
        .map(|i| Array1::from_vec(vec![(i as f64) * 0.1; n]))
        .collect();
    let deltas: Vec<f64> = vec![0.025; 16];
    let h0 = Array1::zeros(n);

    // Fused tiled scan (discretize per tile, keep in cache)
    let tiled_states = TiledScan::fused_tiled_scan(&a, &b_shared, &deltas, &h0, 4);
    println!(
        "  Fused tiled scan: {} states of dim {}",
        tiled_states.len(),
        n
    );

    // Verify against sequential
    let mut h_seq = h0.clone();
    for t in 0..16 {
        let (a_bar, b_bar) = ssm::discretize_zoh(&a, &b_shared[t], deltas[t]);
        h_seq = a_bar.dot(&h_seq) + &b_bar;
        let diff = (&tiled_states[t] - &h_seq).mapv(|x| x.abs()).sum();
        assert!(diff < 1e-10, "Tiled scan mismatch at t={}: {}", t, diff);
    }
    println!("  Sequential match: ✓\n");

    // ── 5. Grouped Gating (GQA-style) ──────────────────────────────────
    println!("─── 5. Grouped Gating ───");
    let gqa_config = MambaModelConfig {
        vocab_size: 100,
        n_layers: 1,
        d_model: 32,
        state_dim: 4,
        expand_factor: 2, // d_inner = 64
        conv_kernel: 3,
        eps: 1e-5,
        weight_tied: false,
        gate_type: GateType::Grouped { num_groups: 4 }, // gate_dim = 64/4 = 16
    };
    let gqa_model = MambaModel::new(gqa_config);
    let gqa_logits = gqa_model.forward(&vec![5, 10, 15]);
    println!(
        "  Grouped gating (4 groups): [{}, {}] ✓",
        gqa_logits.shape()[0],
        gqa_logits.shape()[1]
    );
    println!(
        "  Gate dim = {}, d_inner = {}",
        gqa_model.layers[0].gate_dim, gqa_model.layers[0].d_inner
    );
    println!("  Parameters: ~{}\n", gqa_model.param_count());

    // ── 6. CUDA-style parallel scan ────────────────────────────────────
    println!("─── 6. CUDA-Style Parallel Scan ───");
    let elements: Vec<ScanElement> = (0..20)
        .map(|i| ScanElement::new(0.9 - (i as f64 * 0.02), (i as f64) * 0.5))
        .collect();

    let cuda_result = CudaScan::scan(&elements, 8);
    println!("  CUDA scan (block=8): {} elements", cuda_result.len());

    // Verify
    let mut ra = 1.0;
    let mut rb = 0.0;
    let mut all_ok = true;
    for i in 0..20 {
        ra = elements[i].a * ra;
        rb = elements[i].a * rb + elements[i].b;
        if (cuda_result[i].a - ra).abs() > 1e-10 || (cuda_result[i].b - rb).abs() > 1e-10 {
            all_ok = false;
        }
    }
    println!("  Sequential match: {}\n", if all_ok { "✓" } else { "✗" });

    // ── 7. Weight Tying ────────────────────────────────────────────────
    println!("─── 7. Weight Tying ───");
    let tied_config = MambaModelConfig {
        vocab_size: 100,
        n_layers: 1,
        d_model: 16,
        state_dim: 4,
        expand_factor: 2,
        conv_kernel: 3,
        eps: 1e-5,
        weight_tied: true,
        gate_type: GateType::SiLU,
    };
    let tied_model = MambaModel::new(tied_config);
    let tied_logits = tied_model.forward(&vec![5, 10, 15]);
    println!(
        "  Weight-tied model: [{}, {}] ✓",
        tied_logits.shape()[0],
        tied_logits.shape()[1]
    );
    println!(
        "  Params: ~{} (embedding reused as output projection)\n",
        tied_model.param_count()
    );

    println!("═══ All extensions demo complete ═══");
}
