//! Minimal Transformer / Mini LLM for character-level text generation.
//!
//! A compact transformer decoder that can train on tiny corpora (e.g. 1 KB)
//! and generate character-level continuations. Used as a lightweight local
//! language model for simple text tasks.
//!
//! # Architecture
//! - Character-level tokenizer
//! - Embedding → positional encoding → single transformer block → LM head
//! - Tiny: d_model=128, 1 layer, 4 heads, MLP ratio 4x

pub mod mini_llm;
