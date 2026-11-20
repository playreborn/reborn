//! Downloads from Steam's content servers (CDN and caches).
//!
//! URLs (SteamKit2 `Steam/CDN/Client.cs`):
//! - manifest: `/depot/<depot>/manifest/<manifest id>/5/<request code>`
//! - chunk: `/depot/<depot>/chunk/<chunk id as lowercase hex>`
//!
//! The responses are returned as raw bytes; verification happens in
//! [`crate::manifest`] and [`crate::chunk`].

use std::time::Duration;

use crate::manifest::ChunkId;

/// Manifest format version in manifest URLs (SteamKit2 `MANIFEST_VERSION`).
const MANIFEST_VERSION: u32 = 5;
const TIMEOUT: Duration = Duration::from_secs(60);
const USER_AGENT: &str = concat!("reborn/", env!("CARGO_PKG_VERSION"));

/// A content server to download from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CdnHost {
    /// Host name (Steam's `vhost`).
    pub host: String,
    /// Whether to use HTTPS.
    pub https: bool,
}

impl CdnHost {
    fn url(&self, path: &str) -> String {
        let scheme = if self.https { "https" } else { "http" };
        format!("{scheme}://{}{path}", self.host)
    }
}

/// A download failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CdnError {
    /// Network or TLS error, or timeout.
    #[error("request to {host} failed: {source}")]
    Request {
        /// The server.
        host: String,
        /// The underlying error.
        source: reqwest::Error,
    },
    /// The server answered with an HTTP error status.
    #[error("{host} answered with HTTP {status}")]
    Status {
        /// The server.
        host: String,
        /// HTTP status code.
        status: u16,
    },
}

impl CdnError {
    /// Whether trying again (possibly on another server) can help.
    pub fn is_retryable(&self) -> bool {
        match self {
            CdnError::Request { .. } => true,
            CdnError::Status { status, .. } => *status == 429 || *status >= 500,
        }
    }
}

/// HTTP client for content servers. Cheap to clone.
#[derive(Clone, Debug)]
pub struct CdnClient {
    pub(crate) http: reqwest::Client,
}

impl CdnClient {
    /// Creates a client with timeouts and Reborn's user agent.
    pub fn new() -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(TIMEOUT)
            .build()?;
        Ok(Self { http })
    }

    /// Downloads a manifest (still zipped; see [`crate::manifest::DepotManifest::parse`]).
    pub async fn manifest(
        &self,
        server: &CdnHost,
        depot_id: u32,
        manifest_id: u64,
        request_code: u64,
    ) -> Result<Vec<u8>, CdnError> {
        let path = manifest_path(depot_id, manifest_id, request_code);
        self.get(server, &path).await
    }

    /// Downloads a chunk (still encrypted; see [`crate::chunk::process_chunk`]).
    pub async fn chunk(
        &self,
        server: &CdnHost,
        depot_id: u32,
        chunk: &ChunkId,
    ) -> Result<Vec<u8>, CdnError> {
        let path = chunk_path(depot_id, chunk);
        self.get(server, &path).await
    }

    async fn get(&self, server: &CdnHost, path: &str) -> Result<Vec<u8>, CdnError> {
        let request_error = |source| CdnError::Request {
            host: server.host.clone(),
            source,
        };
        let response = self
            .http
            .get(server.url(path))
            .send()
            .await
            .map_err(request_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(CdnError::Status {
                host: server.host.clone(),
                status: status.as_u16(),
            });
        }
        let body = response.bytes().await.map_err(request_error)?;
        Ok(body.to_vec())
    }
}

fn manifest_path(depot_id: u32, manifest_id: u64, request_code: u64) -> String {
    format!("/depot/{depot_id}/manifest/{manifest_id}/{MANIFEST_VERSION}/{request_code}")
}

fn chunk_path(depot_id: u32, chunk: &ChunkId) -> String {
    format!("/depot/{depot_id}/chunk/{}", chunk.to_hex())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_urls_like_steamkit() {
        let https = CdnHost {
            host: "cache1-fra1.steamcontent.com".to_owned(),
            https: true,
        };
        assert_eq!(
            https.url(&manifest_path(481, 3183503801510301321, 42)),
            "https://cache1-fra1.steamcontent.com/depot/481/manifest/3183503801510301321/5/42"
        );

        let http = CdnHost {
            host: "10.0.0.1".to_owned(),
            https: false,
        };
        let chunk = ChunkId([0xab; 20]);
        assert_eq!(
            http.url(&chunk_path(481, &chunk)),
            format!("http://10.0.0.1/depot/481/chunk/{}", "ab".repeat(20))
        );
    }

    #[test]
    fn classifies_retryable_errors() {
        let status = |status| CdnError::Status {
            host: "h".to_owned(),
            status,
        };
        assert!(status(503).is_retryable());
        assert!(status(429).is_retryable());
        assert!(!status(404).is_retryable());
        assert!(!status(403).is_retryable());
    }
}
