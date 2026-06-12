#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};
#[cfg(feature = "cli")]
use std::path::PathBuf;
#[cfg(feature = "cli")]
use avid_anticlone::{fingerprint_submission, check, Submission};
#[cfg(feature = "cli")]
use std::{fs, collections::HashMap};

#[cfg(feature = "cli")]
#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum Commands {
    Check {
        /// File to check
        file: PathBuf,
        /// Corpus directory to compare against
        #[arg(long)]
        corpus: PathBuf,
    },
}

#[cfg(feature = "cli")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Check { file, corpus } => {
            let content = fs::read_to_string(file)?;
            let mut files = HashMap::new();
            files.insert("main.py".to_string(), content);
            let sub = Submission {
                files,
                entrypoint: "main.py".to_string(),
            };
            let fp = fingerprint_submission(&sub)?;

            let mut max_score = 0.0f32;

            if corpus.is_dir() {
                for entry in fs::read_dir(corpus)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "py") {
                        let other_content = fs::read_to_string(&path)?;
                        let mut other_files = HashMap::new();
                        other_files.insert("main.py".to_string(), other_content);
                        let other_sub = Submission {
                            files: other_files,
                            entrypoint: "main.py".to_string(),
                        };
                        if let Ok(other_fp) = fingerprint_submission(&other_sub) {
                            let corpus_item = vec![(0i64, &other_fp)];
                            let report = check(&fp, corpus_item.into_iter(), 0.0);
                            if report.score > max_score {
                                max_score = report.score;
                            }
                        }
                    }
                }
            }

            println!("{}", max_score);
        }
    }
    Ok(())
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("CLI feature not enabled");
    std::process::exit(1);
}
