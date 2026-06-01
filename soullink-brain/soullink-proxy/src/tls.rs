//! TLS certificate management — self-signed auto-generation with rcgen.

use anyhow::Result;
use rcgen::{CertificateParams, DistinguishedName, KeyPair, SanType};
use std::path::Path;

/// Generate a self-signed TLS certificate for SoulLink proxy.
pub fn generate_self_signed_cert() -> Result<(String, String)> {
    let mut params = CertificateParams::default();
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "SoulLink V13.5 Proxy");
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, "SoulLink");

    // SANs: localhost + local IPs
    params.subject_alt_names = vec![
        SanType::DnsName("localhost".try_into()?),
        SanType::IpAddress("127.0.0.1".parse()?),
        SanType::IpAddress("0.0.0.0".parse()?),
    ];

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    Ok((cert.pem(), key_pair.serialize_pem()))
}

/// Save certificate and key to files.
pub fn save_cert_files(
    cert_path: &Path,
    key_path: &Path,
    cert_pem: &str,
    key_pem: &str,
) -> Result<()> {
    std::fs::write(cert_path, cert_pem)?;
    std::fs::write(key_path, key_pem)?;
    Ok(())
}

/// Load or generate TLS cert/key pair.
/// If files exist, load them. Otherwise, generate and save.
pub fn load_or_generate_tls(cert_path: &Path, key_path: &Path) -> Result<(Vec<u8>, Vec<u8>)> {
    if cert_path.exists() && key_path.exists() {
        let cert = std::fs::read(cert_path)?;
        let key = std::fs::read(key_path)?;
        return Ok((cert, key));
    }

    let (cert_pem, key_pem) = generate_self_signed_cert()?;

    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    save_cert_files(cert_path, key_path, &cert_pem, &key_pem)?;
    Ok((cert_pem.into_bytes(), key_pem.into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn generate_self_signed_cert_works() {
        let (cert, key) = generate_self_signed_cert().unwrap();
        assert!(cert.contains("BEGIN CERTIFICATE"));
        assert!(key.contains("BEGIN"));
    }

    #[test]
    fn save_and_load_cert_files() {
        let dir = std::env::temp_dir().join("soullink-proxy-test");
        fs::create_dir_all(&dir).unwrap();

        let cert_path = dir.join("test-cert.pem");
        let key_path = dir.join("test-key.pem");

        let (cert, key) = generate_self_signed_cert().unwrap();
        save_cert_files(&cert_path, &key_path, &cert, &key).unwrap();

        assert!(cert_path.exists());
        assert!(key_path.exists());

        let loaded = load_or_generate_tls(&cert_path, &key_path).unwrap();
        assert!(!loaded.0.is_empty());
        assert!(!loaded.1.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
