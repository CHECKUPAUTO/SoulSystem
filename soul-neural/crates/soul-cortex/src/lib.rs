use soul_embed::Embedder;
use soul_minillm::MiniLLM;
use ndarray::Array2;
use anyhow::Result;
pub mod symplectic_bias;

/// Un cortex unifiant dynamique hamiltonienne et génération LLM.
pub struct Cortex {
    hnn: soul_hnn::HamiltonianNN,
    llm: MiniLLM,
    embedder: Embedder,
    #[allow(dead_code)]
    state_projection: Array2<f64>,
    #[allow(dead_code)]
    token_bias_weight: f64,
}

impl Cortex {
    pub fn new(
        hnn: soul_hnn::HamiltonianNN,
        llm: MiniLLM,
        embedder: Embedder,
    ) -> Self {
        let dim_hnn = hnn.state_dim();
        let dim_llm = llm.latent_dim().expect("LLM must expose latent dim");
        let state_projection = Array2::zeros((dim_hnn, dim_llm));
        Cortex { hnn, llm, embedder, state_projection, token_bias_weight: 0.1 }
    }

    pub async fn perceive(&mut self, stimulus: &str) -> Result<()> {
        let embedding = self.embedder.embed(stimulus).await?;
        self.hnn.apply_impulse(&embedding);
        Ok(())
    }

    pub async fn generate(&mut self, prompt: &str, max_tokens: usize) -> Result<String> {
        let prompt_embedding = self.embedder.embed(prompt).await?;
        self.hnn.apply_impulse(&prompt_embedding);

        let mut generated = String::new();
        let mut current_input = prompt.to_string();
        for _ in 0..max_tokens {
            let logits = self.llm.logits(&current_input).await?;
            let biased_logits = self.apply_symplectic_bias(logits)?;
            let token = self.llm.sample_token(&biased_logits)?;
            let token_str = self.llm.decode_token(token)?;
            if token_str == "<eos>" { break; }
            generated.push_str(&token_str);
            current_input.push_str(&token_str);
            let token_embed = self.embedder.embed(&token_str).await?;
            self.hnn.apply_impulse(&token_embed);
        }
        Ok(generated)
    }

    fn apply_symplectic_bias(&self, logits: Vec<f64>) -> Result<Vec<f64>> {
        let state = self.hnn.get_state_vector();
        let dim = state.len() / 2;
        let q: Vec<f64> = state[..dim].to_vec();
        let p: Vec<f64> = state[dim..].to_vec();
        Ok(symplectic_bias::apply_symplectic_bias(logits, &q, &p))
    }
}
