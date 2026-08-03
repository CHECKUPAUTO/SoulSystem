//! Import command for migrating data from other AI systems.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Subcommand;

#[cfg(feature = "import")]
use crate::import::ImportOptions;
#[cfg(feature = "import")]
use crate::import::legacy_agent::AgentImporter;

/// Import data from other AI systems.
#[derive(Subcommand, Debug, Clone)]
pub enum ImportCommand {
    /// Import from Agent (memory, history, settings, credentials)
    #[cfg(feature = "import")]
    Agent {
        /// Path to Agent directory (default: ~/.agent)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Dry-run mode: show what would be imported without writing
        #[arg(long)]
        dry_run: bool,

        /// Re-embed memory if dimensions don't match target provider
        #[arg(long)]
        re_embed: bool,

        /// User ID for imported data (default: 'default')
        #[arg(long)]
        user_id: Option<String>,
    },
}

/// Run an import command.
#[cfg(feature = "import")]
pub async fn run_import_command(
    cmd: &ImportCommand,
    config: &crate::config::Config,
) -> anyhow::Result<()> {
    match cmd {
        ImportCommand::Agent {
            path,
            dry_run,
            re_embed,
            user_id,
        } => run_import_agent(config, path.clone(), *dry_run, *re_embed, user_id.clone()).await,
    }
}

/// Run the Agent import.
#[cfg(feature = "import")]
async fn run_import_agent(
    config: &crate::config::Config,
    agent_path: Option<PathBuf>,
    dry_run: bool,
    re_embed: bool,
    user_id: Option<String>,
) -> anyhow::Result<()> {
    use secrecy::SecretString;

    // Determine Agent path
    let agent_path = if let Some(path) = agent_path {
        path
    } else if let Some(path) = AgentImporter::detect() {
        path
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".agent")
    };

    let user_id = user_id.unwrap_or_else(|| "default".to_string());

    println!("🔍 Agent Import");
    println!("  Path: {}", agent_path.display());
    println!("  User: {}", user_id);
    if dry_run {
        println!("  Mode: DRY RUN (no data will be written)");
    }
    println!();

    // Initialize database
    let db = crate::db::connect_from_config(&config.database)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize database: {}", e))?;

    // Initialize secrets store with master key from env or keychain
    let secrets_crypto = if let Ok(master_key_hex) = std::env::var("SECRETS_MASTER_KEY") {
        Arc::new(
            crate::secrets::SecretsCrypto::new(SecretString::from(master_key_hex))
                .map_err(|e| anyhow::anyhow!("Failed to initialize secrets: {}", e))?,
        )
    } else {
        match crate::secrets::keychain::get_master_key().await {
            Ok(key_bytes) => {
                let key_hex: String = key_bytes.iter().map(|b| format!("{:02x}", b)).collect();
                Arc::new(
                    crate::secrets::SecretsCrypto::new(SecretString::from(key_hex))
                        .map_err(|e| anyhow::anyhow!("Failed to initialize secrets: {}", e))?,
                )
            }
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "No secrets master key found. Set SECRETS_MASTER_KEY env var or run 'soulsystem onboard' first."
                ));
            }
        }
    };

    let secrets: Arc<dyn crate::secrets::SecretsStore> = Arc::new(
        crate::secrets::InMemorySecretsStore::new(secrets_crypto.clone()),
    );

    // Initialize workspace
    let workspace = crate::workspace::Workspace::new_with_db(user_id.clone(), db.clone());

    let opts = ImportOptions {
        agent_path,
        dry_run,
        re_embed,
        user_id,
    };

    let importer = AgentImporter::new(db, workspace, secrets, opts);
    let stats = importer.import().await?;

    // Print results
    println!("Import Complete");
    println!();
    println!("Summary:");
    println!("  Documents:    {}", stats.documents);
    println!("  Chunks:       {}", stats.chunks);
    println!("  Conversations: {}", stats.conversations);
    println!("  Messages:     {}", stats.messages);
    println!("  Settings:     {}", stats.settings);
    println!("  Secrets:      {}", stats.secrets);
    if stats.skipped > 0 {
        println!("  Skipped:      {}", stats.skipped);
    }
    if stats.re_embed_queued > 0 {
        println!("  Re-embed queued: {}", stats.re_embed_queued);
    }
    println!();
    println!("Total imported: {}", stats.total_imported());

    if dry_run {
        println!();
        println!("[DRY RUN] No data was written.");
    }

    Ok(())
}

#[cfg(not(feature = "import"))]
pub async fn run_import_command(
    _cmd: &ImportCommand,
    _config: &crate::config::Config,
) -> anyhow::Result<()> {
    anyhow::bail!("Import feature not enabled. Compile with --features import")
}
