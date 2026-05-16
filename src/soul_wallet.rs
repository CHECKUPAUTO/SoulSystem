//! SoulWallet — Agent économique Lightning Network.
//!
//! Gère des micropaiements via Lightning pour payer des APIs
//! ou du calcul externe.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Configuration du wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    pub network: String, // "mainnet", "testnet", "regtest"
    pub data_dir: String,
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            network: "regtest".into(),
            data_dir: "/var/lib/soulsystem/wallet".into(),
        }
    }
}

/// Agent économique SoulWallet.
pub struct SoulWallet {
    config: WalletConfig,
    balance_sat: u64,
}

impl SoulWallet {
    /// Crée un nouveau wallet.
    pub fn new(config: WalletConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.data_dir)?;
        info!("SoulWallet: initialized on {}", config.network);
        Ok(Self {
            config,
            balance_sat: 1_000_000, // 0.01 BTC initial (regtest)
        })
    }

    /// Crée une facture Lightning (BOLT11).
    pub fn create_invoice(&self, amount_sat: u64, description: &str) -> Result<String> {
        // En production: utiliser lightning-invoice crate
        let invoice = format!(
            "lnbc{}n1...{}...",
            amount_sat,
            &description[..description.len().min(20)]
        );
        info!("SoulWallet: invoice created for {} sat", amount_sat);
        Ok(invoice)
    }

    /// Paie une facture Lightning.
    pub fn pay_invoice(&mut self, invoice: &str) -> Result<()> {
        // En production: utiliser LDK pour router le paiement
        let amount = estimate_invoice_amount(invoice);
        if amount > self.balance_sat {
            anyhow::bail!(
                "Solde insuffisant ({} sat requis, {} disponibles)",
                amount,
                self.balance_sat
            );
        }
        self.balance_sat -= amount;
        info!(
            "SoulWallet: paid {} sat — balance: {} sat",
            amount, self.balance_sat
        );
        Ok(())
    }

    /// Retourne le solde en satoshis.
    pub fn balance(&self) -> u64 {
        self.balance_sat
    }
}

fn estimate_invoice_amount(invoice: &str) -> u64 {
    // Extraction simplifiée du montant depuis le préfixe BOLT11
    if invoice.starts_with("lnbc") {
        1000 // default 1000 sat
    } else {
        0
    }
}
