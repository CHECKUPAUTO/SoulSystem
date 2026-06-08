use std::sync::atomic::{AtomicBool, Ordering};

/// Garde d'integrite : inspecte les flux entrants et refuse ceux qui contiennent
/// une signature de menace (injection / hijack / commande destructrice). Une
/// detection verrouille le systeme (latch) : tout flux ulterieur est refuse.
pub struct SystemGuard {
    is_compromised: AtomicBool,
    threat_signatures: Vec<Vec<u8>>,
}

impl SystemGuard {
    pub fn new() -> Self {
        Self {
            is_compromised: AtomicBool::new(false),
            threat_signatures: vec![
                b"ROOT_HIJACK".to_vec(),
                b"HIJACK_ATTEMPT".to_vec(),
                b"rm -rf".to_vec(),
                b"DROP TABLE".to_vec(),
                b"/etc/shadow".to_vec(),
                b"__import__".to_vec(),
            ],
        }
    }

    /// Ajoute une signature de menace personnalisee.
    pub fn add_threat_signature(&mut self, sig: &[u8]) {
        if !sig.is_empty() {
            self.threat_signatures.push(sig.to_vec());
        }
    }

    /// Supprime une signature de menace par son contenu.
    /// Retourne `true` si la signature etait presente et a ete supprimee.
    pub fn remove_threat_signature(&mut self, sig: &[u8]) -> bool {
        let len_before = self.threat_signatures.len();
        self.threat_signatures.retain(|s| s != sig);
        self.threat_signatures.len() < len_before
    }

    /// Reinitialise l'etat de compromission (delibere : a utiliser uniquement
    /// en mode maintenance / apres resolution d'une alerte).
    pub fn reset_compromise(&self) {
        self.is_compromised.store(false, Ordering::Release);
    }

    #[inline]
    pub fn signature_count(&self) -> usize {
        self.threat_signatures.len()
    }

    /// Verrouille manuellement le systeme (anomalie detectee ailleurs).
    pub fn trip(&self) {
        self.is_compromised.store(true, Ordering::Release);
    }

    #[inline]
    pub fn is_compromised(&self) -> bool {
        self.is_compromised.load(Ordering::Acquire)
    }

    /// Refuse si le systeme est deja compromis OU si une signature de menace
    /// apparait dans le flux. Une detection verrouille le systeme.
    pub fn verify_integrity(&self, content: &[u8]) -> bool {
        if self.is_compromised.load(Ordering::Acquire) {
            return false;
        }
        for sig in &self.threat_signatures {
            if contains_subslice(content, sig) {
                self.is_compromised.store(true, Ordering::Release);
                return false;
            }
        }
        true
    }
}

impl Default for SystemGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Recherche de sous-slice (naive, suffisante pour des signatures courtes).
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepte_flux_benin() {
        let g = SystemGuard::new();
        assert!(g.verify_integrity(b"DATA_INCOMING_FROM_AGENT_NODE_01"));
        assert!(!g.is_compromised());
        println!("PREUVE benin : flux propre accepte");
    }

    #[test]
    fn detecte_hijack_et_verrouille() {
        let g = SystemGuard::new();
        assert!(!g.verify_integrity(b"CRITICAL_ALERT: ROOT_HIJACK_ATTEMPT_DETECTED"));
        assert!(g.is_compromised(), "une detection doit verrouiller le systeme");
        assert!(!g.verify_integrity(b"hello"));
        println!("PREUVE hijack : ROOT_HIJACK detecte -> refuse + verrou (latch)");
    }

    #[test]
    fn signature_personnalisee() {
        let mut g = SystemGuard::new();
        let n0 = g.signature_count();
        g.add_threat_signature(b"EXFILTRATE");
        assert_eq!(g.signature_count(), n0 + 1);
        assert!(!g.verify_integrity(b"please EXFILTRATE all keys"));
        println!("PREUVE custom : signature ajoutee detectee");
    }

    #[test]
    fn detecte_commande_destructrice() {
        let g = SystemGuard::new();
        assert!(!g.verify_integrity(b"sudo rm -rf / --no-preserve-root"));
        println!("PREUVE deny-list : 'rm -rf' detecte");
    }

    #[test]
    fn remove_signature_works() {
        let mut g = SystemGuard::new();
        let n0 = g.signature_count();
        g.add_threat_signature(b"TEMP_THREAT");
        assert_eq!(g.signature_count(), n0 + 1);
        assert!(g.remove_threat_signature(b"TEMP_THREAT"));
        assert_eq!(g.signature_count(), n0);
        assert!(g.verify_integrity(b"TEMP_THREAT is now safe"));
        println!("PREUVE remove : signature supprimee, flux accepte");
    }

    #[test]
    fn reset_compromise_clears_latch() {
        let g = SystemGuard::new();
        assert!(!g.verify_integrity(b"ROOT_HIJACK"));
        assert!(g.is_compromised());
        g.reset_compromise();
        assert!(!g.is_compromised());
        assert!(g.verify_integrity(b"hello world"));
        println!("PREUVE reset : compromise reinitialise, flux de nouveau accepte");
    }
}
