//! Domain-specific error types for `iopool2mqtt`.

use thiserror::Error;

/// Central error type returned by `iopool2mqtt` library operations.
#[derive(Debug, Error)]
pub enum Error {
    /// Missing or invalid configuration parameter.
    #[error("configuration error for `{key}`: {message}")]
    Config {
        /// Name of the configuration variable or field.
        key: &'static str,
        /// Explanation of why the configuration value is invalid.
        message: String,
    },

    /// Failure while constructing or parsing an endpoint URL.
    #[error("invalid URL `{url}`: {message}")]
    InvalidUrl {
        /// The raw URL string that failed to parse.
        url: String,
        /// Explanation of the URL parse error.
        message: String,
    },

    /// HTTP transport or request failure when calling the iopool API.
    #[error("iopool HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Unexpected HTTP status code returned by the iopool API.
    #[error("iopool API returned unexpected HTTP status {status}: {body}")]
    ApiStatus {
        /// HTTP status code.
        status: u16,
        /// Truncated response body for diagnostics.
        body: String,
    },

    /// JSON serialization or deserialization error.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// MQTT client error when publishing or disconnecting.
    #[error("MQTT client error: {0}")]
    MqttClient(#[from] rumqttc::ClientError),

    /// MQTT connection error from the event loop.
    #[error("MQTT connection error: {0}")]
    MqttConnection(String),
}

/// Convenience result alias using [`enum@Error`].
pub type Result<T> = std::result::Result<T, Error>;
