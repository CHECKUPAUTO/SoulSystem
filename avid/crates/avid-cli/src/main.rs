use std::sync::Arc;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "avid")]
#[command(about = "AVID Digital Organism CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Starts the AVID API server
    Server,
    /// Starts the AVID interactive TUI
    Tui,
    /// Navigate to a URL with headless browser and get rendered HTML
    Navigate {
        /// URL to navigate to
        url: String,
        /// Wait for a CSS selector before returning (optional)
        #[arg(short, long)]
        wait_for: Option<String>,
        /// Timeout in milliseconds (default: 15000)
        #[arg(short, long, default_value = "15000")]
        timeout: u64,
        /// Take a screenshot instead of returning HTML
        #[arg(short, long)]
        screenshot: bool,
        /// Output file for screenshot (auto-generated if not specified)
        #[arg(short, long)]
        output: Option<String>,
        /// Viewport width (default: 1920)
        #[arg(long, default_value = "1920")]
        width: u32,
        /// Viewport height (default: 1080)
        #[arg(long, default_value = "1080")]
        height: u32,
        /// Fill form input: selector=value (can be repeated). Uses Angular-compatible dispatch.
        #[arg(short = 'f', long = "fill", num_args = 1)]
        fill: Vec<String>,
        /// Click on a CSS selector after page load
        #[arg(short = 'c', long = "click", num_args = 1)]
        click: Vec<String>,
        /// Evaluate JavaScript expression after page load
        #[arg(short = 'e', long = "eval")]
        eval_js: Option<String>,
        /// Wait in ms after fill/click operations (default: 1000)
        #[arg(long = "wait-ms", default_value = "1000")]
        wait_ms: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Server => {
            avid_server::run().await?;
        }
        Commands::Tui => {
            avid_tui::run().await.map_err(|e| anyhow::anyhow!(e))?;
        }
        Commands::Navigate {
            url,
            wait_for,
            timeout,
            screenshot,
            output,
            width,
            height,
            fill,
            click,
            eval_js,
            wait_ms,
        } => {
            use avid_scout::headless::HeadlessBrowser;

            let browser = Arc::new(HeadlessBrowser::launch().await?);

            let session = browser.new_session(width, height).await?;

            // Navigate
            let _html = session.fetch_rendered(&url, timeout).await?;

            // Execute custom JS before fill/click
            // Security: block obvious Node.js/Deno require patterns in eval'd JS
            if let Some(ref js) = eval_js {
                if js.contains("require(") || js.contains("import ") || js.contains("process.") {
                    eprintln!("[eval] BLOCKED: eval JS contains require/import/process access");
                } else {
                    let r = session.eval(js).await?;
                    eprintln!("[eval] {r}");
                }
            }

            // Fill form fields with real CDP keystrokes (Angular-compatible)
            for f in &fill {
                let parts: Vec<&str> = f.splitn(2, '=').collect();
                if parts.len() == 2 {
                    let selector = parts[0];
                    let value = parts[1];
                    session.type_into(selector, value).await?;
                }
            }

            // Wait for Angular to update form state
            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;

            // Click elements
            for sel in &click {
                let js = format!(
                    "document.querySelector('{}').click();",
                    sel.replace('\'', "\\'")
                );
                session.eval(&js).await?;
            }

            // Wait for selector if specified
            if let Some(sel) = &wait_for {
                session.wait_for(sel, timeout).await?;
            }

            // Small grace for rendering after actions
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;

            if screenshot {
                let bytes = session.screenshot(true).await?;
                let path = output.unwrap_or_else(|| {
                    let slug = url
                        .replace("https://", "")
                        .replace("http://", "")
                        .replace(|c: char| !c.is_alphanumeric() && c != '.', "_");
                    format!("screenshot_{slug}.png")
                });
                std::fs::write(&path, &bytes)?;
                println!("Screenshot saved: {path}");
                println!("Size: {} bytes", bytes.len());
            } else {
                println!("{_html}");
            }
        }
    }

    Ok(())
}
