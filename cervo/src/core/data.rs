use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Compteur logique monotone servant d'horodatage déterministe pour la
/// provenance des `Data` (évite la non-reproductibilité d'une horloge murale
/// dans les tests). Chaque donnée émise reçoit un `created_at` croissant.
static DATA_SEQ: AtomicU64 = AtomicU64::new(1);

/// Prochain numéro de séquence logique (utilisé comme `created_at`).
pub fn next_seq() -> u64 {
    DATA_SEQ.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Data {
    pub payload: Vec<u8>,
    pub metadata: DataMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DataMetadata {
    pub created_at: u64,
    pub source_unit: Option<Uuid>,
    pub tags: Vec<String>,
    pub mime_type: Option<String>,
}

impl Data {
    pub fn new(payload: Vec<u8>) -> Self {
        Data {
            payload,
            metadata: DataMetadata::default(),
        }
    }

    pub fn with_metadata(mut self, metadata: DataMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn tag(mut self, tag: &str) -> Self {
        self.metadata.tags.push(tag.to_string());
        self
    }

    /// Crée une donnée horodatée et attribuée à l'unité `source` : `created_at`
    /// reçoit un numéro de séquence logique croissant, `source_unit` l'identité
    /// productrice. Sert à tracer la provenance dans l'écosystème.
    pub fn stamped(payload: Vec<u8>, source: Uuid) -> Self {
        Data {
            payload,
            metadata: DataMetadata {
                created_at: next_seq(),
                source_unit: Some(source),
                tags: Vec::new(),
                mime_type: None,
            },
        }
    }

    /// Marque la provenance d'une donnée existante (attribution + horodatage
    /// logique si absent). Idempotent sur `created_at` déjà positionné.
    pub fn with_source(mut self, source: Uuid) -> Self {
        self.metadata.source_unit = Some(source);
        if self.metadata.created_at == 0 {
            self.metadata.created_at = next_seq();
        }
        self
    }

    /// Renseigne le type MIME logique de la donnée.
    pub fn with_mime(mut self, mime: &str) -> Self {
        self.metadata.mime_type = Some(mime.to_string());
        self
    }
}

impl From<Vec<u8>> for Data {
    fn from(payload: Vec<u8>) -> Self {
        Data::new(payload)
    }
}
