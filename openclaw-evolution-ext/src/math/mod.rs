/// Modules mathématiques pour le système d'apprentissage OpenClaw/SoulLink
///
/// 🔴 Priorité 1 — Impact immédiat HNN
pub mod hessian;      // #1 Hessienne complète H_ij = ∂²U/∂q_i∂q_j
pub mod optimizer;    // #2 Adam / AdamW adaptatif
pub mod compression;  // #3 SVD tronquée A ≈ U_r Σ_r V_r^T
///
/// 🟡 Priorité 2 — Évolution naturelle
pub mod langevin;     // #4 SDE dp = -∇U dt + σ dW (Euler-Maruyama)
pub mod attention;    // #5 Softmax attention attn = softmax(QK^T/√d)V
pub mod rl;           // #6 PPO L_CLIP
pub mod contrastive;  // #7 Contrastive loss (InfoNCE)
///
/// 🟢 Priorité 3 — Architecture future
pub mod moe;          // #8 Mixture of Experts gating
pub mod quantize;     // #9 Quantization 3-bit (TurboQuant style)
pub mod graph;        // #10 GNN message passing
