//! Application configuration loaded from environment variables and `.env` files.
//!
//! System environment variables always take precedence over values defined in `.env`.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use rumqttc::QoS;

use crate::error::{Error, Result};

/// Default base URL for the iopool public REST API.
pub const DEFAULT_IOPOOL_API_URL: &str = "https://api.iopool.com/v1";
/// Default HTTP timeout in seconds for iopool API calls.
pub const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 15;
/// Default MQTT broker hostname.
pub const DEFAULT_MQTT_HOST: &str = "localhost";
/// Default MQTT broker port.
pub const DEFAULT_MQTT_PORT: u16 = 1883;
/// Default MQTT client identifier.
pub const DEFAULT_MQTT_CLIENT_ID: &str = "iopool2mqtt";
/// Default root MQTT topic prefix.
pub const DEFAULT_MQTT_BASE_TOPIC: &str = "iopool2mqtt";
/// Default flag for using pool `title` instead of `id` in MQTT topics and HA sensor names.
pub const DEFAULT_MQTT_USE_POOL_TITLE: bool = false;
/// Default MQTT QoS level (1 = AtLeastOnce).
pub const DEFAULT_MQTT_QOS: u8 = 1;
/// Default MQTT retain flag for published pool states and metrics.
pub const DEFAULT_MQTT_RETAIN: bool = true;
/// Default MQTT keep-alive interval in seconds.
pub const DEFAULT_MQTT_KEEP_ALIVE_SECS: u64 = 30;
/// Default Home Assistant discovery activation state.
pub const DEFAULT_HA_DISCOVERY_ENABLED: bool = true;
/// Default Home Assistant MQTT discovery prefix.
pub const DEFAULT_HA_DISCOVERY_PREFIX: &str = "homeassistant";
/// Default polling interval in seconds (5 minutes).
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 300;
/// Default single-run flag (`false` = run continuously as a daemon).
pub const DEFAULT_RUN_ONCE: bool = false;

/// Validated runtime configuration for `iopool2mqtt`.
#[derive(Clone, PartialEq, Eq)]
pub struct Config {
    /// Personal API key used to authenticate against the iopool API (`x-api-key` header).
    pub iopool_api_key: String,
    /// Base URL of the iopool API (e.g. `https://api.iopool.com/v1`).
    pub iopool_api_url: String,
    /// Optional explicit list of pool IDs to poll. When empty, `/v1/pools` is queried to fetch all pools.
    pub iopool_pool_ids: Vec<String>,
    /// HTTP timeout for requests made to the iopool API.
    pub iopool_http_timeout: Duration,
    /// Hostname or IP address of the MQTT broker.
    pub mqtt_host: String,
    /// TCP port of the MQTT broker.
    pub mqtt_port: u16,
    /// Optional username for MQTT broker authentication.
    pub mqtt_username: Option<String>,
    /// Optional password for MQTT broker authentication.
    pub mqtt_password: Option<String>,
    /// MQTT client ID used when connecting to the broker.
    pub mqtt_client_id: String,
    /// Base MQTT topic prefix (without trailing slash), e.g. `iopool2mqtt`.
    pub mqtt_base_topic: String,
    /// Whether to use the pool's `title` instead of its `id` in MQTT topic paths and Home Assistant sensor names.
    pub mqtt_use_pool_title: bool,
    /// MQTT Quality of Service level (`0`, `1`, or `2`).
    pub mqtt_qos: QoS,
    /// Whether published pool messages should have the MQTT `retain` flag set.
    pub mqtt_retain: bool,
    /// MQTT keep-alive interval.
    pub mqtt_keep_alive: Duration,
    /// Whether Home Assistant MQTT Auto-Discovery payloads should be published.
    pub ha_discovery_enabled: bool,
    /// Topic prefix for Home Assistant MQTT Auto-Discovery (usually `homeassistant`).
    pub ha_discovery_prefix: String,
    /// Interval between consecutive polling cycles in daemon mode.
    pub poll_interval: Duration,
    /// If `true`, poll once, publish all metrics, and exit immediately.
    pub run_once: bool,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("iopool_api_key", &"[REDACTED]")
            .field("iopool_api_url", &self.iopool_api_url)
            .field("iopool_pool_ids", &self.iopool_pool_ids)
            .field("iopool_http_timeout", &self.iopool_http_timeout)
            .field("mqtt_host", &self.mqtt_host)
            .field("mqtt_port", &self.mqtt_port)
            .field("mqtt_username", &self.mqtt_username)
            .field(
                "mqtt_password",
                &self.mqtt_password.as_ref().map(|_| "[REDACTED]"),
            )
            .field("mqtt_client_id", &self.mqtt_client_id)
            .field("mqtt_base_topic", &self.mqtt_base_topic)
            .field("mqtt_use_pool_title", &self.mqtt_use_pool_title)
            .field("mqtt_qos", &self.mqtt_qos)
            .field("mqtt_retain", &self.mqtt_retain)
            .field("mqtt_keep_alive", &self.mqtt_keep_alive)
            .field("ha_discovery_enabled", &self.ha_discovery_enabled)
            .field("ha_discovery_prefix", &self.ha_discovery_prefix)
            .field("poll_interval", &self.poll_interval)
            .field("run_once", &self.run_once)
            .finish()
    }
}

impl Config {
    /// Loads configuration from `.env` (if present) and process environment variables.
    ///
    /// Existing environment variables are never overwritten by `.env` entries.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if `IOPOOL_API_KEY` is missing or any variable has an invalid format.
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Loads configuration from an arbitrary key-value map (primarily useful for deterministic unit testing).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if required keys are missing or invalid.
    pub fn from_map(map: &HashMap<&str, &str>) -> Result<Self> {
        Self::from_lookup(|key| map.get(key).map(|v| (*v).to_owned()))
    }

    /// Internal parser using a generic lookup closure.
    fn from_lookup<F>(lookup: F) -> Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let iopool_api_key = lookup("IOPOOL_API_KEY")
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::Config {
                key: "IOPOOL_API_KEY",
                message: "missing required environment variable IOPOOL_API_KEY".to_owned(),
            })?;

        let iopool_api_url = lookup_non_empty(&lookup, "IOPOOL_API_URL")
            .unwrap_or_else(|| DEFAULT_IOPOOL_API_URL.to_owned())
            .trim_end_matches('/')
            .to_owned();

        // Validate that the base URL parses cleanly
        reqwest::Url::parse(&iopool_api_url).map_err(|err| Error::InvalidUrl {
            url: iopool_api_url.clone(),
            message: err.to_string(),
        })?;

        let iopool_pool_ids = lookup_non_empty(&lookup, "IOPOOL_POOL_IDS")
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let http_timeout_secs = parse_u64(
            &lookup,
            "IOPOOL_HTTP_TIMEOUT_SECS",
            DEFAULT_HTTP_TIMEOUT_SECS,
        )?;
        if http_timeout_secs == 0 {
            return Err(Error::Config {
                key: "IOPOOL_HTTP_TIMEOUT_SECS",
                message: "timeout must be greater than 0 seconds".to_owned(),
            });
        }

        let mqtt_host =
            lookup_non_empty(&lookup, "MQTT_HOST").unwrap_or_else(|| DEFAULT_MQTT_HOST.to_owned());

        let mqtt_port = parse_u16(&lookup, "MQTT_PORT", DEFAULT_MQTT_PORT)?;
        if mqtt_port == 0 {
            return Err(Error::Config {
                key: "MQTT_PORT",
                message: "port must be between 1 and 65535".to_owned(),
            });
        }

        let mqtt_username = lookup_non_empty(&lookup, "MQTT_USERNAME");
        let mqtt_password = lookup_non_empty(&lookup, "MQTT_PASSWORD");

        let mqtt_client_id = lookup_non_empty(&lookup, "MQTT_CLIENT_ID")
            .unwrap_or_else(|| DEFAULT_MQTT_CLIENT_ID.to_owned());

        let mqtt_base_topic = normalize_topic(
            &lookup_non_empty(&lookup, "MQTT_BASE_TOPIC")
                .unwrap_or_else(|| DEFAULT_MQTT_BASE_TOPIC.to_owned()),
            "MQTT_BASE_TOPIC",
        )?;

        let mqtt_use_pool_title =
            parse_bool(&lookup, "MQTT_USE_POOL_TITLE", DEFAULT_MQTT_USE_POOL_TITLE)?;

        let qos_raw = parse_u8(&lookup, "MQTT_QOS", DEFAULT_MQTT_QOS)?;
        let mqtt_qos = match qos_raw {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            other => {
                return Err(Error::Config {
                    key: "MQTT_QOS",
                    message: format!("invalid QoS level `{other}`, expected 0, 1, or 2"),
                });
            }
        };

        let mqtt_retain = parse_bool(&lookup, "MQTT_RETAIN", DEFAULT_MQTT_RETAIN)?;
        let keep_alive_secs = parse_u64(
            &lookup,
            "MQTT_KEEP_ALIVE_SECS",
            DEFAULT_MQTT_KEEP_ALIVE_SECS,
        )?;
        if keep_alive_secs == 0 {
            return Err(Error::Config {
                key: "MQTT_KEEP_ALIVE_SECS",
                message: "keep-alive must be greater than 0 seconds".to_owned(),
            });
        }

        let ha_discovery_enabled = parse_bool(
            &lookup,
            "HA_DISCOVERY_ENABLED",
            DEFAULT_HA_DISCOVERY_ENABLED,
        )?;

        let ha_discovery_prefix = normalize_topic(
            &lookup_non_empty(&lookup, "HA_DISCOVERY_PREFIX")
                .unwrap_or_else(|| DEFAULT_HA_DISCOVERY_PREFIX.to_owned()),
            "HA_DISCOVERY_PREFIX",
        )?;

        let poll_interval_secs =
            parse_u64(&lookup, "POLL_INTERVAL_SECS", DEFAULT_POLL_INTERVAL_SECS)?;
        if poll_interval_secs == 0 {
            return Err(Error::Config {
                key: "POLL_INTERVAL_SECS",
                message: "poll interval must be greater than 0 seconds".to_owned(),
            });
        }

        let run_once = parse_bool(&lookup, "RUN_ONCE", DEFAULT_RUN_ONCE)?;

        Ok(Self {
            iopool_api_key,
            iopool_api_url,
            iopool_pool_ids,
            iopool_http_timeout: Duration::from_secs(http_timeout_secs),
            mqtt_host,
            mqtt_port,
            mqtt_username,
            mqtt_password,
            mqtt_client_id,
            mqtt_base_topic,
            mqtt_use_pool_title,
            mqtt_qos,
            mqtt_retain,
            mqtt_keep_alive: Duration::from_secs(keep_alive_secs),
            ha_discovery_enabled,
            ha_discovery_prefix,
            poll_interval: Duration::from_secs(poll_interval_secs),
            run_once,
        })
    }
}

fn lookup_non_empty<F>(lookup: &F, key: &str) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key)
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn normalize_topic(raw: &str, key: &'static str) -> Result<String> {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(Error::Config {
            key,
            message: "topic prefix cannot be empty".to_owned(),
        });
    }
    if trimmed.contains('#') || trimmed.contains('+') {
        return Err(Error::Config {
            key,
            message: "topic prefix cannot contain MQTT wildcard characters ('+' or '#')".to_owned(),
        });
    }
    Ok(trimmed.to_owned())
}

fn parse_u64<F>(lookup: &F, key: &'static str, default: u64) -> Result<u64>
where
    F: Fn(&str) -> Option<String>,
{
    match lookup_non_empty(lookup, key) {
        Some(val) => val.parse::<u64>().map_err(|_| Error::Config {
            key,
            message: format!("expected an unsigned integer, got `{val}`"),
        }),
        None => Ok(default),
    }
}

fn parse_u16<F>(lookup: &F, key: &'static str, default: u16) -> Result<u16>
where
    F: Fn(&str) -> Option<String>,
{
    match lookup_non_empty(lookup, key) {
        Some(val) => val.parse::<u16>().map_err(|_| Error::Config {
            key,
            message: format!("expected a 16-bit unsigned integer (1..65535), got `{val}`"),
        }),
        None => Ok(default),
    }
}

fn parse_u8<F>(lookup: &F, key: &'static str, default: u8) -> Result<u8>
where
    F: Fn(&str) -> Option<String>,
{
    match lookup_non_empty(lookup, key) {
        Some(val) => val.parse::<u8>().map_err(|_| Error::Config {
            key,
            message: format!("expected an 8-bit unsigned integer, got `{val}`"),
        }),
        None => Ok(default),
    }
}

fn parse_bool<F>(lookup: &F, key: &'static str, default: bool) -> Result<bool>
where
    F: Fn(&str) -> Option<String>,
{
    match lookup_non_empty(lookup, key) {
        Some(val) => match val.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(Error::Config {
                key,
                message: format!("expected boolean (`true`/`false`/`1`/`0`), got `{val}`"),
            }),
        },
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults_with_minimal_env() {
        let mut env = HashMap::new();
        env.insert("IOPOOL_API_KEY", "secret-key-123");

        let cfg = Config::from_map(&env).unwrap();
        assert_eq!(cfg.iopool_api_key, "secret-key-123");
        assert_eq!(cfg.iopool_api_url, DEFAULT_IOPOOL_API_URL);
        assert!(cfg.iopool_pool_ids.is_empty());
        assert_eq!(
            cfg.iopool_http_timeout,
            Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS)
        );
        assert_eq!(cfg.mqtt_host, DEFAULT_MQTT_HOST);
        assert_eq!(cfg.mqtt_port, DEFAULT_MQTT_PORT);
        assert_eq!(cfg.mqtt_username, None);
        assert_eq!(cfg.mqtt_password, None);
        assert_eq!(cfg.mqtt_client_id, DEFAULT_MQTT_CLIENT_ID);
        assert_eq!(cfg.mqtt_base_topic, DEFAULT_MQTT_BASE_TOPIC);
        assert!(!cfg.mqtt_use_pool_title);
        assert_eq!(cfg.mqtt_qos, QoS::AtLeastOnce);
        assert!(cfg.mqtt_retain);
        assert!(cfg.ha_discovery_enabled);
        assert_eq!(cfg.ha_discovery_prefix, DEFAULT_HA_DISCOVERY_PREFIX);
        assert_eq!(
            cfg.poll_interval,
            Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS)
        );
        assert!(!cfg.run_once);
    }

    #[test]
    fn test_config_missing_api_key_fails() {
        let env = HashMap::new();
        let err = Config::from_map(&env).unwrap_err();
        assert!(matches!(
            err,
            Error::Config {
                key: "IOPOOL_API_KEY",
                ..
            }
        ));
    }

    #[test]
    fn test_config_custom_overrides_and_pool_ids() {
        let mut env = HashMap::new();
        env.insert("IOPOOL_API_KEY", "my-key");
        env.insert("IOPOOL_API_URL", "https://api.iopool.com/v1/");
        env.insert("IOPOOL_POOL_IDS", " pool-1 , pool-2 , ");
        env.insert("IOPOOL_HTTP_TIMEOUT_SECS", "30");
        env.insert("MQTT_HOST", "nanomq");
        env.insert("MQTT_PORT", "1884");
        env.insert("MQTT_USERNAME", "mqtt_user");
        env.insert("MQTT_PASSWORD", "mqtt_pass");
        env.insert("MQTT_CLIENT_ID", "custom_client");
        env.insert("MQTT_BASE_TOPIC", "/custom/iopool/");
        env.insert("MQTT_USE_POOL_TITLE", "true");
        env.insert("MQTT_QOS", "2");
        env.insert("MQTT_RETAIN", "false");
        env.insert("MQTT_KEEP_ALIVE_SECS", "60");
        env.insert("HA_DISCOVERY_ENABLED", "0");
        env.insert("HA_DISCOVERY_PREFIX", "/ha_custom/");
        env.insert("POLL_INTERVAL_SECS", "120");
        env.insert("RUN_ONCE", "yes");

        let cfg = Config::from_map(&env).unwrap();
        assert_eq!(cfg.iopool_api_url, "https://api.iopool.com/v1");
        assert_eq!(cfg.iopool_pool_ids, vec!["pool-1", "pool-2"]);
        assert_eq!(cfg.iopool_http_timeout, Duration::from_secs(30));
        assert_eq!(cfg.mqtt_host, "nanomq");
        assert_eq!(cfg.mqtt_port, 1884);
        assert_eq!(cfg.mqtt_username.as_deref(), Some("mqtt_user"));
        assert_eq!(cfg.mqtt_password.as_deref(), Some("mqtt_pass"));
        assert_eq!(cfg.mqtt_client_id, "custom_client");
        assert_eq!(cfg.mqtt_base_topic, "custom/iopool");
        assert!(cfg.mqtt_use_pool_title);
        assert_eq!(cfg.mqtt_qos, QoS::ExactlyOnce);
        assert!(!cfg.mqtt_retain);
        assert_eq!(cfg.mqtt_keep_alive, Duration::from_secs(60));
        assert!(!cfg.ha_discovery_enabled);
        assert_eq!(cfg.ha_discovery_prefix, "ha_custom");
        assert_eq!(cfg.poll_interval, Duration::from_secs(120));
        assert!(cfg.run_once);
    }

    #[test]
    fn test_config_redacts_secrets_in_debug() {
        let mut env = HashMap::new();
        env.insert("IOPOOL_API_KEY", "super-secret-api-key");
        env.insert("MQTT_PASSWORD", "super-secret-mqtt-pass");

        let cfg = Config::from_map(&env).unwrap();
        let debug_str = format!("{cfg:?}");
        assert!(!debug_str.contains("super-secret-api-key"));
        assert!(!debug_str.contains("super-secret-mqtt-pass"));
        assert!(debug_str.contains("[REDACTED]"));
    }

    #[test]
    fn test_config_invalid_values_rejected() {
        let mut env = HashMap::new();
        env.insert("IOPOOL_API_KEY", "key");
        env.insert("MQTT_QOS", "5");
        assert!(matches!(
            Config::from_map(&env),
            Err(Error::Config {
                key: "MQTT_QOS",
                ..
            })
        ));

        env.insert("MQTT_QOS", "1");
        env.insert("MQTT_BASE_TOPIC", "iopool/+/state");
        assert!(matches!(
            Config::from_map(&env),
            Err(Error::Config {
                key: "MQTT_BASE_TOPIC",
                ..
            })
        ));
    }
}
