//! TLS for the HTTP clients: rustls with the pure-Rust RustCrypto provider (no `ring`, no C),
//! and the bundled webpki root certificates. The provider is set per agent, so nothing depends
//! on a process-wide default.

use std::sync::Arc;

/// The TLS settings every HTTPS agent in this crate uses.
pub fn config() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .root_certs(ureq::tls::RootCerts::WebPki)
        .unversioned_rustls_crypto_provider(Arc::new(rustls_rustcrypto::provider()))
        .build()
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_agent_with_the_pure_rust_provider_builds_and_refuses_a_closed_port() {
        let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(super::config()).build().into();
        // Nothing listens on port 1; the point is that building the TLS stack does not panic.
        assert!(agent.get("https://127.0.0.1:1/").call().is_err());
    }
}
