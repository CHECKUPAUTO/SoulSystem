#![allow(
    clippy::cast_precision_loss,
    clippy::needless_collect,
    clippy::suboptimal_flops,
    clippy::manual_pattern_char_comparison,
    clippy::if_not_else,
    clippy::bool_to_int_with_if,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// WHOIS lookup result.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WhoisInfo {
    pub registrar: Option<String>,
    pub creation_date: Option<String>,
    pub expiration_date: Option<String>,
    pub name_servers: Vec<String>,
    pub raw: String,
}

/// Perform a WHOIS lookup via TCP port 43.
pub fn lookup_whois(domain: &str) -> Result<WhoisInfo, std::io::Error> {
    let server = whois_server_for(domain);
    let mut stream = TcpStream::connect((server.as_str(), 43))?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(format!("{domain}\r\n").as_bytes())?;

    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;

    let registrar = extract_whois_field(&raw, "Registrar:");
    let creation = extract_whois_field(&raw, "Creation Date:")
        .or_else(|| extract_whois_field(&raw, "Created:"));
    let expiration = extract_whois_field(&raw, "Registry Expiry Date:")
        .or_else(|| extract_whois_field(&raw, "Expires:"));

    let name_servers: Vec<String> = raw
        .lines()
        .filter(|l| {
            l.to_lowercase().starts_with("name server:")
                || l.to_lowercase().starts_with("nameserver:")
        })
        .filter_map(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .collect();

    Ok(WhoisInfo {
        registrar,
        creation_date: creation,
        expiration_date: expiration,
        name_servers,
        raw,
    })
}

fn whois_server_for(domain: &str) -> String {
    let tld = domain.rsplit('.').next().unwrap_or("com").to_lowercase();
    match tld.as_str() {
        "fr" => "whois.nic.fr".to_string(),
        "de" => "whois.denic.de".to_string(),
        "uk" => "whois.nic.uk".to_string(),
        "io" => "whois.nic.io".to_string(),
        _ => "whois.iana.org".to_string(),
    }
}

fn extract_whois_field(raw: &str, key: &str) -> Option<String> {
    raw.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
}
