pub mod captcha;
pub mod cli;
pub mod config;
pub mod proxy;
pub mod scanner;
pub mod utils;

// Ré-exportations pour les tests d'intégration
pub use captcha::CaptchaTask;
pub use config::GomogoConfig;
pub use proxy::run_proxy;
pub use scanner::run_scanner;
pub use utils::{format_size, random_user_agent, truncate};
