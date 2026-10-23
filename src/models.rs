//! Data models representing iopool API responses and outgoing MQTT messages.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Represents a swimming pool or spa returned by the iopool Public API (`/v1/pools` or `/v1/pool/{id}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pool {
    /// Unique identifier of the pool (UUID).
    pub id: String,
    /// Human-readable name of the pool (e.g. `"Piscine"`).
    pub title: String,
    /// Current operating mode of the pool (e.g. `"STANDARD"`, `"OPENING"`, `"WINTER"`, `"INITIALIZATION"`).
    #[serde(default)]
    pub mode: Option<String>,
    /// Indicates whether the user needs to perform a maintenance or treatment action.
    #[serde(default)]
    pub has_an_action_required: Option<bool>,
    /// Most recent measurement captured by the EcO probe, if available.
    #[serde(default)]
    pub latest_measure: Option<LatestMeasure>,
    /// Filtration and maintenance advice computed by iopool.
    #[serde(default)]
    pub advice: Option<Advice>,
    /// Optional salt level information (may be `null`, a number, or an object depending on pool type).
    #[serde(default)]
    pub salt: Option<Value>,
    /// Captures any additional fields returned by future versions of the iopool API.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Pool {
    /// Returns the MQTT topic segment identifying this pool:
    /// either `self.id` (when `use_pool_title` is `false`), or a sanitized version of `self.title`
    /// (when `use_pool_title` is `true` and `self.title` is non-empty).
    #[must_use]
    pub fn topic_segment(&self, use_pool_title: bool) -> String {
        if use_pool_title {
            let sanitized = sanitize_topic_segment(&self.title);
            if !sanitized.is_empty() {
                return sanitized;
            }
        }
        self.id.clone()
    }
}

/// Sanitizes a human-readable string (such as `pool.title`) so it is safe to use as a single MQTT topic segment.
#[must_use]
pub fn sanitize_topic_segment(raw: &str) -> String {
    raw.trim()
        .chars()
        .map(|c| {
            if c == '/' || c == '+' || c == '#' || c == '\0' || c.is_whitespace() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// Represents the latest water quality measurement from an iopool EcO sensor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestMeasure {
    /// Water temperature in degrees Celsius (°C).
    #[serde(default)]
    pub temperature: Option<f64>,
    /// Potential of Hydrogen (pH) level.
    #[serde(default)]
    pub ph: Option<f64>,
    /// Oxidation-Reduction Potential (ORP / Redox) in millivolts (mV).
    #[serde(default)]
    pub orp: Option<f64>,
    /// Measurement transmission mode (e.g. `"gateway"`, `"standard"`, `"bluetooth"`).
    #[serde(default)]
    pub mode: Option<String>,
    /// Whether the sensor reading is considered valid by iopool.
    #[serde(default)]
    pub is_valid: Option<bool>,
    /// Identifier of the EcO probe that took the measurement.
    #[serde(default)]
    pub eco_id: Option<String>,
    /// Identifier of the iopool Wi-Fi gateway, when relayed via a gateway.
    #[serde(default)]
    pub gateway_code: Option<String>,
    /// ISO-8601 UTC timestamp of when the measurement was recorded.
    #[serde(default)]
    pub measured_at: Option<String>,
    /// Captures any additional fields returned by future versions of the iopool API.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Filtration recommendations for a pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Advice {
    /// Recommended daily filtration duration in hours.
    #[serde(default)]
    pub filtration_duration: Option<f64>,
    /// Captures any additional fields returned by future versions of the iopool API.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Represents a single MQTT message ready to be published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MqttMessage {
    /// Full MQTT topic path (e.g. `iopool2mqtt/cfd5a936.../latestMeasure/temperature`).
    pub topic: String,
    /// UTF-8 string payload.
    pub payload: String,
    /// Whether the broker should retain this message.
    pub retain: bool,
}

impl MqttMessage {
    /// Creates a new [`MqttMessage`].
    #[must_use]
    pub fn new(topic: impl Into<String>, payload: impl Into<String>, retain: bool) -> Self {
        Self {
            topic: topic.into(),
            payload: payload.into(),
            retain,
        }
    }
}

/// Formats a floating-point number cleanly: integers without `.0` if exact, or full float representation.
#[must_use]
pub fn format_number(val: f64) -> String {
    if val.fract() == 0.0 && val.abs() < 1e15 {
        format!("{val:.0}")
    } else {
        val.to_string()
    }
}

/// Formats a generic JSON [`Value`] as a scalar string or compact JSON for individual sub-topics.
#[must_use]
pub fn format_json_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_POOL_JSON: &str = r#"{
        "id": "cfd5a936-e8e6-4ad3-b985-512bee0d9ad9",
        "title": "Piscine",
        "latestMeasure": {
            "temperature": 16.025036640276937,
            "ph": 8.351476506360877,
            "orp": 741,
            "mode": "gateway",
            "isValid": true,
            "ecoId": "0C6rTwUH",
            "gatewayCode": "I20-98cdaceb5d60",
            "measuredAt": "2026-10-23T09:08:00.000Z"
        },
        "mode": "STANDARD",
        "hasAnActionRequired": true,
        "advice": {
            "filtrationDuration": 2
        },
        "salt": null
    }"#;

    #[test]
    fn test_deserialize_real_iopool_payload() {
        let pool: Pool = serde_json::from_str(SAMPLE_POOL_JSON).unwrap();
        assert_eq!(pool.id, "cfd5a936-e8e6-4ad3-b985-512bee0d9ad9");
        assert_eq!(pool.title, "Piscine");
        assert_eq!(pool.mode.as_deref(), Some("STANDARD"));
        assert_eq!(pool.has_an_action_required, Some(true));
        assert_eq!(pool.salt, None);

        let measure = pool.latest_measure.as_ref().unwrap();
        assert!((measure.temperature.unwrap() - 16.025036640276937).abs() < 1e-12);
        assert!((measure.ph.unwrap() - 8.351476506360877).abs() < 1e-12);
        assert_eq!(measure.orp, Some(741.0));
        assert_eq!(measure.mode.as_deref(), Some("gateway"));
        assert_eq!(measure.is_valid, Some(true));
        assert_eq!(measure.eco_id.as_deref(), Some("0C6rTwUH"));
        assert_eq!(measure.gateway_code.as_deref(), Some("I20-98cdaceb5d60"));
        assert_eq!(
            measure.measured_at.as_deref(),
            Some("2026-10-23T09:08:00.000Z")
        );

        let advice = pool.advice.as_ref().unwrap();
        assert_eq!(advice.filtration_duration, Some(2.0));
    }

    #[test]
    fn test_deserialize_minimal_pool_and_extra_fields() {
        let raw = r#"{
            "id": "pool-min",
            "title": "Spa",
            "futureField": "hello"
        }"#;
        let pool: Pool = serde_json::from_str(raw).unwrap();
        assert_eq!(pool.id, "pool-min");
        assert_eq!(pool.title, "Spa");
        assert!(pool.latest_measure.is_none());
        assert_eq!(
            pool.extra.get("futureField"),
            Some(&Value::String("hello".to_owned()))
        );
    }
}
