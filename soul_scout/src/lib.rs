use std::io::{Read, Write};
use std::net::TcpStream;

pub struct SovereignScout {
    target_host: String,
    target_port: u16,
}

impl SovereignScout {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            target_host: host.to_string(),
            target_port: port,
        }
    }

    /// Envoie une requête de recherche brute à l'instance locale de manière synchrone/non-bloquante pour l'OS
    pub fn query_search(&self, query: &str, response_buffer: &mut [u8]) -> std::io::Result<usize> {
        let address = format!("{}:{}", self.target_host, self.target_port);
        let mut stream = TcpStream::connect(address)?;

        // Construction du payload HTTP brut optimisé pour SearXNG
        let request = format!(
            "GET /search?q={}&format=json HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            query, self.target_host
        );

        stream.write_all(request.as_bytes())?;
        stream.flush()?;

        // Lecture directe dans le buffer de perception de l'OS (Zéro allocation globale)
        let bytes_read = stream.read(response_buffer)?;
        Ok(bytes_read)
    }
}
