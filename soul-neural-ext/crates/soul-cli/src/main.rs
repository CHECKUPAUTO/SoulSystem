use soul_cortex::Cortex;
use soul_hnn::HamiltonianNN;
use soul_minillm::MiniLLM;
use soul_embed::Embedder;
use soul_identity::Identity;
use soul_memory_store::SharedVectorStore;
use std::sync::Arc;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("--- SoulNeural System 3 CLI ---");

    let embedder = Embedder::new(&["system", "goal", "reasoning"]);
    let hnn = HamiltonianNN::new(128);
    let llm = MiniLLM::new(&["soul", "neural", "is", "active"]);
    let store = Arc::new(SharedVectorStore::new(128, 10));

    let mut cortex = Cortex::new(hnn, llm, embedder.clone());
    let _identity = Identity::new(embedder, store); // Keep identity alive for its memory side effects

    let mut rl = DefaultEditor::new()?;
    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                if line.trim() == "exit" { break; }
                rl.add_history_entry(line.as_str())?;

                cortex.perceive(&line).await?;
                let response = cortex.generate(&line, 10).await?;
                println!("Cortex: {}", response);
            },
            Err(ReadlineError::Interrupted) => break,
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}
