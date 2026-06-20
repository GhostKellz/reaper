//! Shared HTTP clients with sane timeouts.
//!
//! All network access in reap goes through these helpers so a slow or dead
//! mirror cannot hang the process indefinitely. Without a timeout, `reqwest`
//! waits forever, which is a poor experience during install/upgrade.

use std::sync::LazyLock;
use std::time::Duration;

/// Maximum time to establish a TCP/TLS connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum time for the whole request (connect + transfer).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared, connection-pooled async client.
///
/// Falls back to a default client if the TLS backend fails to initialize, so
/// this never panics.
pub fn client() -> reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default()
    });
    CLIENT.clone()
}

/// Build a blocking client with the same timeouts.
///
/// A fresh client is returned each call because `reqwest::blocking::Client`
/// spins up its own runtime and must not be initialized inside an async
/// context; constructing it on demand keeps that decision with the caller.
pub fn blocking_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .unwrap_or_default()
}
