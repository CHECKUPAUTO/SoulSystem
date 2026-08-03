pub mod attention; // #5 Softmax attention attn = softmax(QK^T/√d)V
pub mod compression; // #3 SVD tronquée A ≈ U_r Σ_r V_r^T
pub mod contrastive; // #7 Contrastive loss (InfoNCE)
pub mod graph;
/// Modules mathématiques pour le système d'apprentissage SoulSystem/SoulLink
///
/// 🔴 Priorité 1 — Impact immédiat HNN
pub mod hessian; // #1 Hessienne complète H_ij = ∂²U/∂q_i∂q_j
///
/// 🟡 Priorité 2 — Évolution naturelle
pub mod langevin; // #4 SDE dp = -∇U dt + σ dW (Euler-Maruyama)
///
/// 🟢 Priorité 3 — Architecture future
pub mod moe; // #8 Mixture of Experts gating
pub mod optimizer; // #2 Adam / AdamW adaptatif
pub mod quantize; // #9 Quantization 3-bit (TurboQuant style)
pub mod rl; // #6 PPO L_CLIP // #10 GNN message passing
