use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub struct SovereignScout {
    target_host: String,
    target_port: u16,
    timeout: Duration,
}

impl SovereignScout {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            target_host: host.to_string(),
            target_port: port,
            timeout: Duration::from_secs(5),
        }
    }

    /// Configure le timeout de connexion/lecture (defaut : 5s).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Envoie une requete de recherche brute a l'instance locale de maniere
    /// synchrone avec timeout configurable.
    pub fn query_search(&self, query: &str, response_buffer: &mut [u8]) -> std::io::Result<usize> {
        let address = format!("{}:{}", self.target_host, self.target_port);
        let mut stream = TcpStream::connect(&address)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        let request = format!(
            "GET /search?q={}&format=json HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            query, self.target_host
        );

        stream.write_all(request.as_bytes())?;
        stream.flush()?;

        let bytes_read = stream.read(response_buffer)?;
        Ok(bytes_read)
    }

    /// Retourne l'hote cible.
    pub fn host(&self) -> &str {
        &self.target_host
    }

    /// Retourne le port cible.
    pub fn port(&self) -> u16 {
        self.target_port
    }
}
