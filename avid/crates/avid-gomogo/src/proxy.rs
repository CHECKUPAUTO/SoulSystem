use crate::cli::ProxyArgs;
use crate::config::{GomogoConfig, ModificationRule, RuleScope};
use crate::utils::truncate;
use anyhow::Context;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair};
use regex::Regex;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};

/// Structure représentant une entrée de log HAR
#[derive(serde::Serialize, Clone)]
struct HarEntry {
    started_date_time: String,
    request: HarRequest,
    response: HarResponse,
    timings: HarTimings,
}

#[derive(serde::Serialize, Clone)]
struct HarRequest {
    method: String,
    url: String,
    headers: Vec<HarHeader>,
    body_size: i64,
}

#[derive(serde::Serialize, Clone)]
struct HarResponse {
    status: i64,
    headers: Vec<HarHeader>,
    body_size: i64,
}

#[derive(serde::Serialize, Clone)]
struct HarHeader {
    name: String,
    value: String,
}

#[derive(serde::Serialize, Clone)]
struct HarTimings {
    send: f64,
    wait: f64,
    receive: f64,
}

#[derive(serde::Serialize)]
struct HarLog {
    version: String,
    creator: HarCreator,
    entries: Vec<HarEntry>,
}

#[derive(serde::Serialize)]
struct HarCreator {
    name: String,
    version: String,
}

pub struct CaMaterial {
    pub cert: CertificateDer<'static>,
    pub key: PrivateKeyDer<'static>,
}

/// Contexte partagé du proxy
struct ProxyContext {
    log_file: Option<Arc<Mutex<std::fs::File>>>,
    whitelist: Vec<Regex>,
    blacklist: Vec<Regex>,
    modification_rules: Vec<ModificationRule>,
    har_entries: Mutex<Vec<HarEntry>>,
    stats: Mutex<ProxyStats>,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
struct ProxyStats {
    total_requests: u64,
    blocked_requests: u64,
    modified_requests: u64,
    bytes_sent: u64,
    bytes_received: u64,
}

pub async fn run_proxy(args: ProxyArgs, config: &GomogoConfig) -> anyhow::Result<()> {
    let cfg = &config.proxy;

    let ca = load_or_generate_ca(
        &args
            .cert_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(&cfg.default_cert_dir)),
    )?;
    let ca = Arc::new(ca);

    let log_file: Option<Arc<Mutex<std::fs::File>>> = match args.output {
        Some(path) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            Some(Arc::new(Mutex::new(file)))
        }
        None => None,
    };

    // Construire les regex de filtrage
    let whitelist: Vec<Regex> = {
        let wl = if !args.whitelist.is_empty() {
            &args.whitelist
        } else {
            &cfg.domain_whitelist
        };
        wl.iter().map(|d| Regex::new(d)).collect::<Result<_, _>>()?
    };

    let blacklist: Vec<Regex> = {
        let bl = if !args.blacklist.is_empty() {
            &args.blacklist
        } else {
            &cfg.domain_blacklist
        };
        bl.iter().map(|d| Regex::new(d)).collect::<Result<_, _>>()?
    };

    let ctx = Arc::new(ProxyContext {
        log_file,
        whitelist,
        blacklist,
        modification_rules: cfg.modification_rules.clone(),
        har_entries: Mutex::new(Vec::new()),
        stats: Mutex::new(ProxyStats::default()),
    });

    let listen = args
        .listen
        .clone()
        .unwrap_or_else(|| cfg.default_listen.clone());
    let addr: SocketAddr = listen.parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!("Proxy MITM en écoute sur {}", addr);

    // Dashboard web optionnel
    if args.dashboard {
        let ctx_dash = ctx.clone();
        let dash_port = args.dashboard_port;
        let har_output = args.har_output.clone();
        std::thread::spawn(move || {
            let rt = actix_web::rt::System::new();
            rt.block_on(async move {
                if let Err(e) = run_dashboard(ctx_dash, dash_port, har_output).await {
                    error!("Erreur dashboard: {}", e);
                }
            });
        });
        info!("Dashboard disponible sur http://127.0.0.1:{}", dash_port);
    }

    loop {
        let (stream, peer) = listener.accept().await?;
        let ca = ca.clone();
        let ctx = ctx.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, &ca, ctx).await {
                error!("Erreur de connexion {}: {}", peer, e);
            }
        });
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    ca: &CaMaterial,
    ctx: Arc<ProxyContext>,
) -> anyhow::Result<()> {
    let mut stream = stream;
    stream.readable().await?;
    let mut peek_buf = [0u8; 4096];
    let n = stream.try_read(&mut peek_buf)?;
    if n == 0 {
        return Ok(());
    }
    let peeked = peek_buf[..n].to_vec();

    if peek_buf[0] == 0x16 {
        let tls_config = build_tls_config(&ca.cert, &ca.key)?;
        let acceptor = TlsAcceptor::from(Arc::new(tls_config));
        let mut chained = peeked;
        let mut rest = Vec::new();
        stream.read_to_end(&mut rest).await?;
        chained.extend_from_slice(&rest);
        let cursor = std::io::Cursor::new(chained);
        let tls_stream = acceptor.accept(cursor).await?;
        let io = TokioIo::new(tls_stream);
        let ctx = ctx.clone();
        http1::Builder::new()
            .serve_connection(io, service_fn(move |req| proxy_service(req, ctx.clone())))
            .await?;
    } else {
        let raw = read_http_request(&mut stream, &peeked).await?;
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(&raw) {
            Ok(httparse::Status::Complete(_)) => {}
            _ => {
                let resp = b"HTTP/1.1 400 Bad Request\r\n\r\n";
                stream.write_all(resp).await?;
                return Ok(());
            }
        }

        if req.method == Some("CONNECT") {
            let target = req.path.unwrap_or("");
            if target.is_empty() {
                let resp = b"HTTP/1.1 400 Bad Request\r\n\r\n";
                stream.write_all(resp).await?;
                return Ok(());
            }

            // Vérifier filtrage domaine pour CONNECT
            let hostname = target.split(':').next().unwrap_or("localhost").to_string();
            if !is_domain_allowed(&ctx, &hostname) {
                let resp = b"HTTP/1.1 403 Forbidden\r\n\r\n";
                stream.write_all(resp).await?;
                ctx.stats.lock().await.blocked_requests += 1;
                return Ok(());
            }

            let resp = b"HTTP/1.1 200 Connection Established\r\n\r\n";
            stream.write_all(resp).await?;

            let (leaf_cert, leaf_key) = generate_leaf_cert(ca, &hostname)?;
            let tls_config = ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![leaf_cert.clone()], leaf_key.clone_key())?;
            let acceptor = TlsAcceptor::from(Arc::new(tls_config));
            let tls_stream = acceptor.accept(stream).await?;
            let io = TokioIo::new(tls_stream);
            let ctx = ctx.clone();
            http1::Builder::new()
                .serve_connection(io, service_fn(move |req| proxy_service(req, ctx.clone())))
                .await?;
        } else {
            let cursor = std::io::Cursor::new(raw);
            let io = TokioIo::new(cursor);
            http1::Builder::new()
                .serve_connection(io, service_fn(move |req| proxy_service(req, ctx.clone())))
                .await?;
        }
    }
    Ok(())
}

async fn proxy_service(
    req: Request<Incoming>,
    ctx: Arc<ProxyContext>,
) -> Result<Response<Full<hyper::body::Bytes>>, hyper::Error> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    // Extraction du host pour filtrage
    let host = uri.host().unwrap_or("").to_string();
    if !is_domain_allowed(&ctx, &host) {
        ctx.stats.lock().await.blocked_requests += 1;
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Full::new(hyper::body::Bytes::from("Blocked by proxy")))
            .unwrap());
    }

    ctx.stats.lock().await.total_requests += 1;

    let full_url = uri.to_string();
    info!("  -> {} {}", method, full_url);
    if let Some(file) = &ctx.log_file {
        let mut f = file.lock().await;
        let _ = writeln!(f, "  -> {method} {full_url}");
    }

    let (_, body) = req.into_parts();
    let mut body_bytes = body
        .collect()
        .await
        .map(|c| c.to_bytes())
        .unwrap_or_default();

    // Appliquer les règles de modification (request)
    body_bytes = apply_rules(&ctx.modification_rules, &body_bytes, RuleScope::Request);

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .no_proxy()
        .build()
        .unwrap();

    let mut req_builder = client.request(method.clone(), &full_url);
    for (name, value) in headers.iter() {
        if name.as_str() != "host" {
            req_builder = req_builder.header(name.as_str(), value.to_str().unwrap_or(""));
        }
    }
    req_builder = req_builder.body(body_bytes.to_vec());

    let start = std::time::Instant::now();
    match req_builder.send().await {
        Ok(res) => {
            let elapsed = start.elapsed();
            let status = res.status();
            let resp_headers = res.headers().clone();
            let mut resp_body = res.text().await.unwrap_or_default();

            // Appliquer les règles de modification (response)
            let resp_bytes = hyper::body::Bytes::from(resp_body.clone());
            let modified = apply_rules(&ctx.modification_rules, &resp_bytes, RuleScope::Response);
            resp_body = String::from_utf8_lossy(&modified).to_string();

            ctx.stats.lock().await.bytes_sent += body_bytes.len() as u64;
            ctx.stats.lock().await.bytes_received += resp_body.len() as u64;

            // Enregistrer dans le HAR
            let har_entry = HarEntry {
                started_date_time: chrono::Utc::now().to_rfc3339(),
                request: HarRequest {
                    method: method.as_str().to_string(),
                    url: full_url.clone(),
                    headers: headers
                        .iter()
                        .map(|(n, v)| HarHeader {
                            name: n.to_string(),
                            value: v.to_str().unwrap_or("").to_string(),
                        })
                        .collect(),
                    body_size: body_bytes.len() as i64,
                },
                response: HarResponse {
                    status: status.as_u16() as i64,
                    headers: resp_headers
                        .iter()
                        .map(|(n, v)| HarHeader {
                            name: n.to_string(),
                            value: v.to_str().unwrap_or("").to_string(),
                        })
                        .collect(),
                    body_size: resp_body.len() as i64,
                },
                timings: HarTimings {
                    send: 0.0,
                    wait: elapsed.as_secs_f64() * 1000.0,
                    receive: 0.0,
                },
            };
            ctx.har_entries.lock().await.push(har_entry);

            info!("  <- {} [{}]", status.as_u16(), resp_body.len());
            if let Some(file) = &ctx.log_file {
                let mut f = file.lock().await;
                let _ = writeln!(f, "  <- {} [{}]", status.as_u16(), resp_body.len());
                if !resp_body.is_empty() {
                    let _ = writeln!(f, "  Body: {}", truncate(&resp_body, 200));
                }
            }

            let mut response = Response::builder().status(status);
            for (name, value) in resp_headers.iter() {
                response = response.header(name, value);
            }
            Ok(response
                .body(Full::new(hyper::body::Bytes::from(resp_body)))
                .unwrap())
        }
        Err(e) => {
            error!("Erreur proxy : {}", e);
            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Full::new(hyper::body::Bytes::from("Proxy error")))
                .unwrap())
        }
    }
}

fn is_domain_allowed(ctx: &ProxyContext, host: &str) -> bool {
    if host.is_empty() {
        return true;
    }
    if !ctx.blacklist.is_empty() && ctx.blacklist.iter().any(|r| r.is_match(host)) {
        return false;
    }
    if !ctx.whitelist.is_empty() && !ctx.whitelist.iter().any(|r| r.is_match(host)) {
        return false;
    }
    true
}

fn apply_rules(rules: &[ModificationRule], data: &[u8], scope: RuleScope) -> hyper::body::Bytes {
    let mut s = String::from_utf8_lossy(data).to_string();
    for rule in rules {
        let apply = matches!(
            (&rule.scope, &scope),
            (RuleScope::Both, _)
                | (RuleScope::Request, RuleScope::Request)
                | (RuleScope::Response, RuleScope::Response)
        );
        if apply {
            if let Ok(re) = Regex::new(&rule.pattern) {
                s = re.replace_all(&s, &rule.replacement).to_string();
            }
        }
    }
    hyper::body::Bytes::from(s)
}

/// Dashboard web du proxy
async fn run_dashboard(
    ctx: Arc<ProxyContext>,
    port: u16,
    har_output: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    use actix_web::{web, App, HttpResponse, HttpServer};

    async fn stats(data: web::Data<Arc<ProxyContext>>) -> HttpResponse {
        let s = data.stats.lock().await.clone();
        HttpResponse::Ok().json(s)
    }

    async fn har_export(
        data: web::Data<Arc<ProxyContext>>,
        har_path: web::Data<Option<std::path::PathBuf>>,
    ) -> HttpResponse {
        let entries = data.har_entries.lock().await.clone();
        let log = HarLog {
            version: "1.2".into(),
            creator: HarCreator {
                name: "GOMOGO".into(),
                version: "0.4.0".into(),
            },
            entries,
        };
        let json = serde_json::to_string_pretty(&log).unwrap_or_default();

        if let Some(ref path) = **har_path {
            let _ = tokio::fs::write(path, &json).await;
        }

        HttpResponse::Ok()
            .content_type("application/json")
            .body(json)
    }

    async fn dashboard_page() -> HttpResponse {
        let html = r#"<!DOCTYPE html>
<html><head><title>GOMOGO Proxy Dashboard</title>
<meta charset="utf-8"><meta http-equiv="refresh" content="3">
<style>
body{font-family:monospace;background:#1a1a2e;color:#e0e0e0;padding:20px}
.card{background:#16213e;border-radius:8px;padding:15px;margin:10px 0}
.stat{display:inline-block;margin:0 20px;text-align:center}
.stat .val{font-size:2em;color:#00d2ff}
h1{color:#00d2ff}
</style></head><body>
<h1>GOMOGO Proxy Dashboard</h1>
<div class="card">
<div class="stat"><div class="val" id="req">-</div>Requêtes</div>
<div class="stat"><div class="val" id="blocked">-</div>Bloquées</div>
<div class="stat"><div class="val" id="up">-</div>↑ Envoyé</div>
<div class="stat"><div class="val" id="down">-</div>↓ Reçu</div>
</div>
<script>
async function poll(){let r=await fetch('/stats');let d=await r.json();
document.getElementById('req').textContent=d.total_requests;
document.getElementById('blocked').textContent=d.blocked_requests;
document.getElementById('up').textContent=(d.bytes_sent/1024).toFixed(1)+' KB';
document.getElementById('down').textContent=(d.bytes_received/1024).toFixed(1)+' KB';}
setInterval(poll,2000);poll();
</script>
</body></html>"#;

        HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(html)
    }

    let addr = format!("127.0.0.1:{port}");
    info!("Dashboard démarré sur http://{}", addr);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::from(ctx.clone()))
            .app_data(web::Data::new(har_output.clone()))
            .route("/", web::get().to(dashboard_page))
            .route("/stats", web::get().to(stats))
            .route("/har", web::get().to(har_export))
    })
    .bind(&addr)?
    .run()
    .await?;

    Ok(())
}

// ─── Fonctions TLS / CA ───

use std::path::PathBuf;

fn load_or_generate_ca(cert_dir: &PathBuf) -> anyhow::Result<CaMaterial> {
    std::fs::create_dir_all(cert_dir)?;
    let ca_cert_path = cert_dir.join("ca-cert.pem");
    let ca_key_path = cert_dir.join("ca-key.pem");

    if ca_cert_path.exists() && ca_key_path.exists() {
        let cert_pem = std::fs::read(&ca_cert_path)?;
        let key_pem = std::fs::read(&ca_key_path)?;
        let certs: Vec<CertificateDer> =
            rustls_pemfile::certs(&mut cert_pem.as_slice()).collect::<Result<Vec<_>, _>>()?;
        let cert = certs.into_iter().next().context("CA cert missing")?;
        let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
            .context("Parsing CA key")?
            .context("CA key missing")?;
        Ok(CaMaterial { cert, key })
    } else {
        let key_pair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let mut params = CertificateParams::default();
        params.alg = &rcgen::PKCS_ECDSA_P256_SHA256;
        params.key_pair = Some(key_pair);
        params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "GOMOGO MITM CA");
        params.distinguished_name = dn;
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
        ];
        let ca_cert = Certificate::from_params(params)?;

        let cert_pem = ca_cert.serialize_pem()?;
        let key_pem = ca_cert.serialize_private_key_pem();
        std::fs::write(&ca_cert_path, &cert_pem)?;
        std::fs::write(&ca_key_path, &key_pem)?;

        let cert_der = ca_cert.serialize_der()?;
        let der_path = cert_dir.join("ca-cert.der");
        std::fs::write(&der_path, &cert_der)?;
        let key_der = ca_cert.serialize_private_key_der();

        Ok(CaMaterial {
            cert: CertificateDer::from(cert_der),
            key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
        })
    }
}

fn generate_leaf_cert(
    ca: &CaMaterial,
    hostname: &str,
) -> anyhow::Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    let leaf_key = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let mut params = CertificateParams::new(vec![hostname.to_string()]);
    params.alg = &rcgen::PKCS_ECDSA_P256_SHA256;
    params.key_pair = Some(leaf_key);
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, hostname);
    params.distinguished_name = dn;

    let leaf_cert = Certificate::from_params(params)?;

    let ca_key_pair = rcgen::KeyPair::from_der_and_sign_algo(
        ca.key.secret_der(),
        &rcgen::PKCS_ECDSA_P256_SHA256,
    )?;
    let mut ca_params = CertificateParams::new(vec![]);
    ca_params.alg = &rcgen::PKCS_ECDSA_P256_SHA256;
    ca_params.key_pair = Some(ca_key_pair);
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let mut ca_dn = DistinguishedName::new();
    ca_dn.push(DnType::CommonName, "GOMOGO MITM CA");
    ca_params.distinguished_name = ca_dn;
    let ca_cert = Certificate::from_params(ca_params)?;

    let leaf_der = leaf_cert.serialize_der_with_signer(&ca_cert)?;
    let leaf_key_der = leaf_cert.serialize_private_key_der();

    Ok((
        CertificateDer::from(leaf_der),
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key_der)),
    ))
}

fn build_tls_config(
    cert: &CertificateDer<'static>,
    key: &PrivateKeyDer<'static>,
) -> anyhow::Result<ServerConfig> {
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key.clone_key())?;
    Ok(config)
}

async fn read_http_request(
    stream: &mut tokio::net::TcpStream,
    initial: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let mut raw = initial.to_vec();
    let mut more = [0u8; 4096];
    loop {
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);
        let status = req.parse(&raw);
        match status {
            Ok(httparse::Status::Complete(_)) => return Ok(raw),
            Err(_) => anyhow::bail!("Invalid HTTP request"),
            _ => {}
        }
        let n = stream.read(&mut more).await?;
        if n == 0 {
            anyhow::bail!("Connection closed during HTTP request");
        }
        raw.extend_from_slice(&more[..n]);
    }
}
