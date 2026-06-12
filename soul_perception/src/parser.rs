use soul_ipc::bus::{AgentMessage, InterAgentBus};

/// Scanner d'extraction lexicale sans allocation (Zero-Copy Slice Processing)
pub struct ZeroCopyScanner<'a> {
    buffer: &'a [u8],
    cursor: usize,
}

impl<'a> ZeroCopyScanner<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Self { buffer, cursor: 0 }
    }

    /// Extrait la prochaine chaîne délimitée par des guillemets (ex: valeurs de clés JSON)
    pub fn next_token(&mut self) -> Option<&'a [u8]> {
        let bytes = self.buffer;
        let mut start = None;

        while self.cursor < bytes.len() {
            if bytes[self.cursor] == b'"' {
                if start.is_none() {
                    self.cursor += 1;
                    start = Some(self.cursor);
                } else if let Some(token_start) = start {
                    let token_end = self.cursor;
                    self.cursor += 1;
                    return Some(&bytes[token_start..token_end]);
                }
            } else {
                self.cursor += 1;
                if self.cursor >= bytes.len() {
                    break;
                }
            }
        }
        None
    }
}

pub struct PerceptionPipeline;

impl PerceptionPipeline {
    /// # Safety
    /// raw_data must be a valid byte slice; ipc_bus must remain alive for the duration of this call.
    pub unsafe fn parse_and_route(
        raw_data: &[u8],
        target_agent_id: u32,
        ipc_bus: &InterAgentBus,
    ) -> usize {
        let mut scanner = ZeroCopyScanner::new(raw_data);
        let mut routed_signals = 0;

        while let Some(token) = scanner.next_token() {
            // Détection de patterns ultra-rapide via hachage ou comparaison de slices
            if token.len() >= 4 && &token[0..4] == b"ERR_" {
                let msg = AgentMessage {
                    source_agent_id: 999, // ID Système Perception
                    target_agent_id,
                    signal_code: 0xEEAA, // Code d'erreur système
                    payload_ptr: token.as_ptr() as *mut u8,
                    payload_size: token.len(),
                };
                if ipc_bus.publish(msg) {
                    routed_signals += 1;
                }
            } else if token.len() >= 5 && &token[0..5] == b"DATA_" {
                let msg = AgentMessage {
                    source_agent_id: 999,
                    target_agent_id,
                    signal_code: 0xDDFF, // Code injection de faits structurés
                    payload_ptr: token.as_ptr() as *mut u8,
                    payload_size: token.len(),
                };
                if ipc_bus.publish(msg) {
                    routed_signals += 1;
                }
            }
        }
        routed_signals
    }
}
