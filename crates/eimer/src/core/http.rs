//! Shared HTTP client. reqwest is built without a bundled crypto provider, so the
//! ring provider is installed here (idempotent) before any client is created.

use reqwest::blocking::{Client, ClientBuilder};

pub const USER_AGENT: &str = concat!("eimer/", env!("CARGO_PKG_VERSION"));

fn install_crypto_provider() {
    // Errors only mean a provider is already installed.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Client builder with the crypto provider installed and eimer's User-Agent set.
pub fn builder() -> ClientBuilder {
    install_crypto_provider();
    Client::builder().user_agent(USER_AGENT)
}

/// Default client. Building only fails if the TLS backend cannot initialise,
/// which is an unrecoverable setup error.
pub fn client() -> Client {
    builder()
        .build()
        .expect("failed to initialise the TLS backend")
}
