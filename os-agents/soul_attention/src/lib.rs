//! KV-cache borne (attention sinks + fenetre glissante) pour decodage long-contexte.
pub mod cache;
pub use cache::KvCache;
pub mod attention;
pub use attention::{attend, attend_into};
