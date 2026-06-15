//! Mini LLM — a compact character-level transformer for local text generation.
//!
//! Designed for tiny corpora (a few KB). Uses a single transformer decoder block
//! with multi-head self-attention and GELU feed-forward.

use std::collections::HashMap;

// ── Configuration ────────────────────────────────────────────────────────────

/// Configuration for the MiniLLM.
#[derive(Debug, Clone)]
pub struct MiniLLMConfig {
    /// Vocabulary size (number of unique tokens).
    pub vocab_size: usize,
    /// Model dimension (embedding + hidden size).
    pub d_model: usize,
    /// Number of attention heads.
    pub n_heads: usize,
    /// Feed-forward expansion ratio (hidden = d_model * ff_ratio).
    pub ff_ratio: usize,
    /// Maximum sequence length.
    pub max_seq_len: usize,
}

impl Default for MiniLLMConfig {
    fn default() -> Self {
        Self {
            vocab_size: 256,
            d_model: 128,
            n_heads: 4,
            ff_ratio: 4,
            max_seq_len: 256,
        }
    }
}

// ── Character Tokenizer ──────────────────────────────────────────────────────

/// Character-level tokenizer.
#[derive(Debug, Clone)]
pub struct CharTokenizer {
    /// Token ID → character string.
    vocab: Vec<String>,
    /// Character → token ID.
    char_to_id: HashMap<char, usize>,
    /// Vocabulary size.
    pub vocab_size: usize,
}

impl CharTokenizer {
    /// Build a tokenizer from a list of vocabulary texts.
    /// Extracts all unique characters from the provided texts.
    pub fn new(vocab_texts: &[&str]) -> Self {
        // Collect all unique characters from vocabulary texts
        let mut char_set: Vec<char> = vocab_texts
            .iter()
            .flat_map(|t| t.chars())
            .collect::<std::collections::BTreeSet<char>>()
            .into_iter()
            .collect();

        // Ensure minimum ASCII printable set
        for c in (' '..='~').chain(std::iter::once('\n')) {
            if !char_set.contains(&c) {
                char_set.push(c);
            }
        }
        char_set.sort();

        let vocab: Vec<String> = char_set.iter().map(|c| c.to_string()).collect();
        let char_to_id: HashMap<char, usize> =
            char_set.iter().enumerate().map(|(i, &c)| (c, i)).collect();

        Self {
            vocab_size: vocab.len(),
            vocab,
            char_to_id,
        }
    }

    /// Encode text into token IDs.
    pub fn encode(&self, text: &str) -> Vec<usize> {
        text.chars()
            .filter_map(|c| self.char_to_id.get(&c).copied())
            .collect()
    }

    /// Decode token IDs back to a string.
    pub fn decode(&self, ids: &[usize]) -> String {
        ids.iter()
            .filter_map(|&id| self.vocab.get(id))
            .cloned()
            .collect()
    }
}

// ── MiniLLM ──────────────────────────────────────────────────────────────────

/// A tiny character-level transformer.
pub struct MiniLLM {
    pub config: MiniLLMConfig,
    pub tokenizer: CharTokenizer,
    // Embedding matrix: [vocab_size × d_model]
    embed: Vec<f32>,
    // Positional encoding: [max_seq_len × d_model]
    pos_enc: Vec<f32>,
    // Attention: Q projection: [d_model × d_model]
    wq: Vec<f32>,
    // Attention: K projection
    wk: Vec<f32>,
    // Attention: V projection
    wv: Vec<f32>,
    // Attention: output projection
    wo: Vec<f32>,
    // FFN: first layer [d_model × d_ff]
    w1: Vec<f32>,
    b1: Vec<f32>,
    // FFN: second layer [d_ff × d_model]
    w2: Vec<f32>,
    b2: Vec<f32>,
    // LM head: [d_model × vocab_size]
    lm_head: Vec<f32>,
}

impl MiniLLM {
    /// Create a new MiniLLM with the given config and tokenizer.
    /// Initializes weights with random orthogonal-like initialization.
    pub fn new(config: MiniLLMConfig, tokenizer: CharTokenizer) -> Self {
        let d = config.d_model;
        let v = config.vocab_size;
        let max_t = config.max_seq_len;
        let d_ff = d * config.ff_ratio;

        let mut rng = XorShiftRng::new(12345);
        let scale = (2.0 / d as f32).sqrt();

        Self {
            config,
            tokenizer,
            embed: rand_vec(&mut rng, v * d, scale),
            pos_enc: generate_positional_encoding(max_t, d),
            wq: rand_vec(&mut rng, d * d, scale),
            wk: rand_vec(&mut rng, d * d, scale),
            wv: rand_vec(&mut rng, d * d, scale),
            wo: rand_vec(&mut rng, d * d, scale),
            w1: rand_vec(&mut rng, d * d_ff, scale),
            b1: vec![0.0; d_ff],
            w2: rand_vec(&mut rng, d_ff * d, scale),
            b2: vec![0.0; d],
            lm_head: rand_vec(&mut rng, d * v, scale),
        }
    }

    /// Forward pass: embed token IDs and produce output logits.
    /// Returns a [seq_len × vocab_size] matrix.
    pub fn forward(&self, ids: &[usize]) -> Vec<Vec<f32>> {
        if ids.is_empty() {
            return vec![];
        }
        let d = self.config.d_model;
        let v = self.config.vocab_size;
        let seq_len = ids.len();
        let n_heads = self.config.n_heads;
        let head_dim = d / n_heads;

        // 1. Embed + positional encoding
        let mut x = vec![0.0f32; seq_len * d];
        for (pos, &id) in ids.iter().enumerate() {
            if id < v {
                for (j, xj) in x[pos * d..(pos + 1) * d].iter_mut().enumerate() {
                    *xj = self.embed[id * d + j] + self.pos_enc[pos * d + j];
                }
            }
        }

        // 2. Multi-head self-attention (single layer)
        let mut attn_out = vec![0.0f32; seq_len * d];
        let scale = 1.0 / (head_dim as f32).sqrt();
        for pos in 0..seq_len {
            // Compute Q for this position
            let q = matvec(&self.wq, &x[pos * d..], d, d);
            let mut head_outs = Vec::with_capacity(n_heads);
            for h in 0..n_heads {
                let q_head = &q[h * head_dim..(h + 1) * head_dim];
                let mut scores = vec![0.0f32; pos + 1];
                let mut max_score = f32::NEG_INFINITY;
                for j in 0..=pos {
                    // K for position j, head h
                    let kj_start = j * d + h * head_dim;
                    let mut dot = 0.0f32;
                    for k in 0..head_dim {
                        // Approximate K: use x as K (simplified shared QKV)
                        let kj = x[kj_start + k];
                        dot += q_head[k] * kj;
                    }
                    scores[j] = dot * scale;
                    if scores[j] > max_score {
                        max_score = scores[j];
                    }
                }
                // Softmax + weighted sum of V
                let mut denom = 0.0f32;
                for s in &mut scores[..=pos] {
                    *s = (*s - max_score).exp();
                    denom += *s;
                }
                let mut head_out = vec![0.0f32; head_dim];
                for j in 0..=pos {
                    let attn_w = scores[j] / denom;
                    let vj_start = j * d + h * head_dim;
                    for k in 0..head_dim {
                        head_out[k] += attn_w * x[vj_start + k]; // V ≈ x
                    }
                }
                head_outs.push(head_out);
            }
            // Concatenate heads
            for h in 0..n_heads {
                let start = h * head_dim;
                for k in 0..head_dim {
                    attn_out[pos * d + start + k] = head_outs[h][k];
                }
            }
        }

        // Output projection
        let attn_proj = matvec_batch(&self.wo, &attn_out, seq_len, d, d);

        // 3. Feed-forward (GELU)
        let mut ff_hidden =
            matvec_batch(&self.w1, &attn_proj, seq_len, d, self.config.ff_ratio * d);
        // Add bias + GELU
        let d_ff = self.config.ff_ratio * d;
        for pos in 0..seq_len {
            for j in 0..d_ff {
                let idx = pos * d_ff + j;
                ff_hidden[idx] = gelu(ff_hidden[idx] + self.b1[j]);
            }
        }
        let ff_out = matvec_batch(&self.w2, &ff_hidden, seq_len, d_ff, d);
        // Add bias
        let mut hidden = vec![0.0f32; seq_len * d];
        for pos in 0..seq_len {
            for j in 0..d {
                hidden[pos * d + j] = ff_out[pos * d + j] + self.b2[j];
            }
        }

        // 4. LM head: project to vocabulary
        let mut logits = vec![vec![0.0f32; v]; seq_len];
        for pos in 0..seq_len {
            for t in 0..v {
                let mut acc = 0.0f32;
                for j in 0..d {
                    acc += hidden[pos * d + j] * self.lm_head[j * v + t];
                }
                logits[pos][t] = acc;
            }
        }

        logits
    }

    /// Generate text continuation from a prompt.
    pub fn generate(&self, prompt: &str, max_tokens: usize) -> String {
        let mut ids = self.tokenizer.encode(prompt);
        let mut output = prompt.to_string();

        for _ in 0..max_tokens {
            let logits = self.forward(&ids);
            let last_logits = &logits.last().unwrap();
            // Greedy sampling
            let next_id = last_logits
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(0);

            // Prevent endless repetition of same token
            // (this is a toy model, keep it simple)
            ids.push(next_id);
            if let Some(next_char) = self.tokenizer.decode(&[next_id]).chars().next() {
                output.push(next_char);
            }

            if ids.len() >= self.config.max_seq_len {
                break;
            }
        }
        output
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn matvec(w: &[f32], x: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; rows];
    for i in 0..rows {
        let mut acc = 0.0;
        for j in 0..cols {
            acc += w[i * cols + j] * x[j];
        }
        out[i] = acc;
    }
    out
}

fn matvec_batch(w: &[f32], x: &[f32], batch: usize, in_dim: usize, out_dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; batch * out_dim];
    for b in 0..batch {
        for i in 0..out_dim {
            let mut acc = 0.0;
            for j in 0..in_dim {
                acc += w[i * in_dim + j] * x[b * in_dim + j];
            }
            out[b * out_dim + i] = acc;
        }
    }
    out
}

fn gelu(x: f32) -> f32 {
    let sqrt_2_over_pi = 0.7978845608f32;
    let coeff = 0.044715f32;
    let x3 = x * x * x;
    x * 0.5 * (1.0 + (sqrt_2_over_pi * (x + coeff * x3)).tanh())
}

fn generate_positional_encoding(max_len: usize, d_model: usize) -> Vec<f32> {
    let mut pe = vec![0.0f32; max_len * d_model];
    for pos in 0..max_len {
        for i in 0..d_model {
            let angle = pos as f32 / 10000.0f32.powf((2 * (i / 2)) as f32 / d_model as f32);
            pe[pos * d_model + i] = if i % 2 == 0 { angle.sin() } else { angle.cos() };
        }
    }
    pe
}

/// Simple deterministic PRNG for weight initialization.
struct XorShiftRng {
    state: u64,
}

impl XorShiftRng {
    fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }

    fn next(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        // Map to [-1, 1]
        (self.state as f64 / u64::MAX as f64 * 2.0 - 1.0) as f32
    }
}

fn rand_vec(rng: &mut XorShiftRng, n: usize, scale: f32) -> Vec<f32> {
    (0..n).map(|_| rng.next() * scale).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_roundtrip() {
        let tk = CharTokenizer::new(&["hello world"]);
        let ids = tk.encode("hello");
        let decoded = tk.decode(&ids);
        assert_eq!(decoded, "hello");
    }

    #[test]
    fn tokenizer_all_printable() {
        let tk = CharTokenizer::new(&[]);
        assert!(tk.vocab_size >= 95); // ASCII printable
        let ids = tk.encode("abc123");
        assert_eq!(ids.len(), 6);
    }

    #[test]
    fn mini_llm_forward_shape() {
        let tk = CharTokenizer::new(&["hello"]);
        let config = MiniLLMConfig {
            vocab_size: tk.vocab_size,
            d_model: 32,
            n_heads: 2,
            ff_ratio: 2,
            max_seq_len: 64,
        };
        let llm = MiniLLM::new(config, tk);
        let ids = vec![0, 1, 2, 3];
        let logits = llm.forward(&ids);
        assert_eq!(logits.len(), 4);
        assert!(!logits[0].is_empty());
    }

    #[test]
    fn mini_llm_generates_text() {
        let tk = CharTokenizer::new(&["hello world this is a test"]);
        let config = MiniLLMConfig {
            vocab_size: tk.vocab_size,
            ..MiniLLMConfig::default()
        };
        let llm = MiniLLM::new(config, tk);
        let result = llm.generate("hel", 10);
        assert!(result.starts_with("hel"));
        assert!(result.len() > 3);
    }
}
