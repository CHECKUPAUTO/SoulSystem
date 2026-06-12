use crate::cli::ScannerArgs;
use crate::config::GomogoConfig;
use crate::utils::{format_size, random_user_agent};
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Client;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use url::Url;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ScanEntry {
    url: String,
    status: u16,
    size: u64,
    content_type: String,
    elapsed_ms: u64,
}

pub async fn run_scanner(args: ScannerArgs, config: &GomogoConfig) -> anyhow::Result<()> {
    let cfg = &config.scanner;

    let mut paths = load_wordlist(&args.wordlist).await?;

    // Support pause/reprise : filtrer les chemins déjà scannés
    if let Some(ref resume_path) = args.resume {
        if resume_path.exists() {
            let already: Vec<ScanEntry> =
                serde_json::from_str(&tokio::fs::read_to_string(resume_path).await?)?;
            let scanned: HashSet<String> = already.into_iter().map(|e| e.url).collect();
            paths.retain(|p| !scanned.contains(p));
            if paths.is_empty() {
                println!("Tous les chemins ont déjà été scannés.");
                return Ok(());
            }
        }
    }

    let concurrency = args.concurrency.unwrap_or(cfg.default_concurrency);
    let _delay_ms = args.delay_ms.unwrap_or(cfg.default_delay_ms);
    let timeout_secs = args.timeout_secs.unwrap_or(cfg.default_timeout_secs);
    let follow_redirects = args.follow_redirects.unwrap_or(cfg.follow_redirects);

    let mut client_builder = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(if follow_redirects {
            reqwest::redirect::Policy::limited(10)
        } else {
            reqwest::redirect::Policy::none()
        })
        .danger_accept_invalid_certs(false);

    // HTTP/2 support
    if args.http2 {
        client_builder = client_builder.http2_prior_knowledge();
    }

    // User-Agent rotation
    let use_rotation = args.rotate_user_agent || cfg.user_agent_rotation;
    if !use_rotation {
        client_builder = client_builder.user_agent("GOMOGO/0.4.0");
    }

    // Proxy upstream pour le scanner
    if let Some(ref proxy_url) = args.proxy {
        client_builder = client_builder.proxy(reqwest::Proxy::all(proxy_url)?);
    }

    let client = client_builder.build()?;

    let mut headers_map = HeaderMap::new();
    for h in &args.headers {
        if let Some((name, value)) = h.split_once(':') {
            headers_map.insert(
                HeaderName::from_str(name.trim())?,
                HeaderValue::from_str(value.trim())?,
            );
        } else {
            eprintln!("Format d'en-tête ignoré : {h}");
        }
    }

    let regex_pattern = args
        .regex_match
        .as_ref()
        .map(|pat| Regex::new(pat))
        .transpose()?;

    let method = args.method.to_uppercase();
    let base = Url::parse(&args.base_url)?;
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

    println!(
        "[Scanner] {} {} sur {} chemins (concurrency={}, http2={}, rotate_ua={})",
        method,
        args.base_url,
        paths.len(),
        concurrency,
        args.http2,
        use_rotation
    );

    let mut handles = Vec::new();
    for path in paths {
        let full_url = base.join(&path)?;
        let client = client.clone();
        let headers = headers_map.clone();
        let method = method.clone();
        let body = args.data.clone();
        let req_regex = regex_pattern.clone();
        let contains = args.contains.clone();
        let require_json = args.require_json;
        let verbose = args.verbose;

        let permit = semaphore.clone().acquire_owned().await?;

        let handle = tokio::spawn(async move {
            let _permit = permit;
            let mut req = match method.as_str() {
                "GET" => client.get(full_url.clone()),
                "POST" => client.post(full_url.clone()),
                "PUT" => client.put(full_url.clone()),
                "DELETE" => client.delete(full_url.clone()),
                "PATCH" => client.patch(full_url.clone()),
                "HEAD" => client.head(full_url.clone()),
                "OPTIONS" => client.request(reqwest::Method::OPTIONS, full_url.clone()),
                _ => {
                    eprintln!("Méthode non supportée : {method}");
                    return None;
                }
            };

            req = req.headers(headers);

            // Appliquer la rotation UA si activée
            if use_rotation {
                req = req.header("User-Agent", random_user_agent());
            }

            if let Some(data) = body {
                req = req.body(data);
            }

            let start = Instant::now();
            match req.send().await {
                Ok(response) => {
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    let status = response.status();
                    let resp_headers = response.headers().clone();
                    let content_len = response.content_length().unwrap_or(0);
                    let content_type = resp_headers
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-")
                        .to_string();
                    let text = response.text().await.unwrap_or_default();

                    let mut show = true;
                    if require_json {
                        show &= serde_json::from_str::<serde_json::Value>(&text).is_ok();
                    }
                    if let Some(ref substr) = contains {
                        show &= text.contains(substr);
                    }
                    if let Some(ref regex) = req_regex {
                        show &= regex.is_match(&text);
                    }

                    if show
                        && (status != reqwest::StatusCode::NOT_FOUND || verbose)
                    {
                        Some(ScanEntry {
                            url: full_url.to_string(),
                            status: status.as_u16(),
                            size: content_len,
                            content_type,
                            elapsed_ms,
                        })
                    } else {
                        None
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        eprintln!("Timeout : {full_url}");
                    }
                    None
                }
            }
        });
        handles.push(handle);
    }

    let mut results: Vec<ScanEntry> = Vec::new();
    for handle in handles {
        if let Ok(Some(entry)) = handle.await {
            let size_str = format_size(entry.size);

            println!(
                "[{}] {:>3} {:<60} [{}]",
                method, entry.status, entry.url, size_str
            );
            results.push(entry);
        }
    }

    // Export JSON
    if let Some(ref json_path) = args.json_output {
        let json_str = serde_json::to_string_pretty(&results)?;
        tokio::fs::write(json_path, json_str).await?;
        println!("Résultats exportés vers {}", json_path.display());
    }

    // Sauvegarde pour pause/reprise
    if let Some(ref resume_path) = args.resume {
        let mut all: Vec<ScanEntry> = if resume_path.exists() {
            serde_json::from_str(&tokio::fs::read_to_string(resume_path).await?)?
        } else {
            vec![]
        };
        all.extend(results.clone());
        let json_str = serde_json::to_string_pretty(&all)?;
        tokio::fs::write(resume_path, json_str).await?;
    }

    println!("Scan terminé. {} réponses affichées.", results.len());
    Ok(())
}

async fn load_wordlist(path: &str) -> anyhow::Result<Vec<String>> {
    let file = File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut paths = Vec::new();
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            paths.push(trimmed);
        }
    }
    Ok(paths)
}
