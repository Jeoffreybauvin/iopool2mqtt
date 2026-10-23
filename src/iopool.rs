//! Asynchronous HTTP client for the iopool Public REST API.
//!
//! Official API documentation: <https://help.iopool.com/fr/articles/5537423-api-publique-iopool>

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use tracing::{debug, instrument};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::models::Pool;

/// Trait abstracting calls to the iopool API to support dependency injection and testing.
#[allow(async_fn_in_trait)]
pub trait IopoolApi: Send + Sync {
    /// Retrieves all pools associated with the authenticated iopool account (`GET /v1/pools`).
    async fn fetch_pools(&self) -> Result<Vec<Pool>>;

    /// Retrieves a single pool by its unique identifier (`GET /v1/pool/{id}`).
    async fn fetch_pool(&self, pool_id: &str) -> Result<Pool>;

    /// Retrieves either the explicitly configured pool IDs (via `/v1/pool/{id}`)
    /// or all pools on the account (via `/v1/pools`) when `pool_ids` is empty.
    async fn fetch_target_pools(&self, pool_ids: &[String]) -> Result<Vec<Pool>> {
        if pool_ids.is_empty() {
            self.fetch_pools().await
        } else {
            let mut pools = Vec::with_capacity(pool_ids.len());
            for id in pool_ids {
                pools.push(self.fetch_pool(id).await?);
            }
            Ok(pools)
        }
    }
}

/// Concrete HTTP client for the iopool Public API backed by [`reqwest::Client`].
#[derive(Debug, Clone)]
pub struct IopoolClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl IopoolClient {
    /// Creates a new [`IopoolClient`] from the application [`Config`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] if the underlying HTTP client cannot be initialized, or
    /// [`Error::InvalidUrl`] if the configured API base URL is malformed.
    pub fn from_config(config: &Config) -> Result<Self> {
        Self::new(
            &config.iopool_api_url,
            &config.iopool_api_key,
            config.iopool_http_timeout,
        )
    }

    /// Creates a new [`IopoolClient`] with explicit parameters.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidUrl`] if `base_url` is not a valid URL, or [`Error::Http`] if
    /// the TLS/HTTP client builder fails.
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        timeout: std::time::Duration,
    ) -> Result<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        url::Url::parse(&base_url).map_err(|source| Error::InvalidUrl {
            url: base_url.clone(),
            source,
        })?;

        let mut default_headers = HeaderMap::new();
        default_headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        let http = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .default_headers(default_headers)
            .build()?;

        Ok(Self {
            http,
            base_url,
            api_key: api_key.into(),
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        debug!(url = %url, "Sending GET request to iopool API");
        let response = self
            .http
            .get(url)
            .header("x-api-key", &self.api_key)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_owned());
            let truncated = if body.len() > 512 {
                format!("{}...", &body[..512])
            } else {
                body
            };
            return Err(Error::ApiStatus {
                status: status.as_u16(),
                body: truncated,
            });
        }

        let parsed = response.json::<T>().await?;
        Ok(parsed)
    }
}

impl IopoolApi for IopoolClient {
    #[instrument(skip(self), fields(base_url = %self.base_url))]
    async fn fetch_pools(&self) -> Result<Vec<Pool>> {
        let url = format!("{}/pools", self.base_url);
        self.get_json(&url).await
    }

    #[instrument(skip(self), fields(base_url = %self.base_url, pool_id = %pool_id))]
    async fn fetch_pool(&self, pool_id: &str) -> Result<Pool> {
        let url = format!("{}/pool/{pool_id}", self.base_url);
        self.get_json(&url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const MOCK_POOL_PAYLOAD: &str = r#"{
        "id": "cfd5a936-e8e6-4ad3-b985-512bee0d9ad9",
        "title": "Piscine",
        "latestMeasure": {
            "temperature": 16.025,
            "ph": 8.35,
            "orp": 741,
            "mode": "gateway",
            "isValid": true,
            "ecoId": "0C6rTwUH",
            "gatewayCode": "I20-98cdaceb5d60",
            "measuredAt": "2026-10-23T09:08:00.000Z"
        },
        "mode": "STANDARD",
        "hasAnActionRequired": true,
        "advice": { "filtrationDuration": 2 },
        "salt": null
    }"#;

    #[tokio::test]
    async fn test_fetch_pools_success() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/pools"))
            .and(header("x-api-key", "test-api-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(format!("[{MOCK_POOL_PAYLOAD}]"), "application/json"),
            )
            .mount(&server)
            .await;

        let client = IopoolClient::new(
            format!("{}/v1", server.uri()),
            "test-api-key",
            Duration::from_secs(5),
        )
        .unwrap();

        let pools = client.fetch_pools().await.unwrap();
        assert_eq!(pools.len(), 1);
        assert_eq!(pools[0].id, "cfd5a936-e8e6-4ad3-b985-512bee0d9ad9");
        assert_eq!(pools[0].title, "Piscine");
    }

    #[tokio::test]
    async fn test_fetch_single_pool_and_target_pools() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/pool/cfd5a936-e8e6-4ad3-b985-512bee0d9ad9"))
            .and(header("x-api-key", "test-api-key"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(MOCK_POOL_PAYLOAD, "application/json"),
            )
            .mount(&server)
            .await;

        let client = IopoolClient::new(
            format!("{}/v1", server.uri()),
            "test-api-key",
            Duration::from_secs(5),
        )
        .unwrap();

        let ids = vec!["cfd5a936-e8e6-4ad3-b985-512bee0d9ad9".to_owned()];
        let pools = client.fetch_target_pools(&ids).await.unwrap();
        assert_eq!(pools.len(), 1);
        assert_eq!(pools[0].title, "Piscine");
    }

    #[tokio::test]
    async fn test_fetch_pools_http_error_status() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/pools"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden: invalid API key"))
            .mount(&server)
            .await;

        let client = IopoolClient::new(
            format!("{}/v1", server.uri()),
            "wrong-key",
            Duration::from_secs(5),
        )
        .unwrap();

        let err = client.fetch_pools().await.unwrap_err();
        assert!(matches!(err, Error::ApiStatus { status: 403, .. }));
    }
}
