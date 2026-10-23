//! Home Assistant MQTT Auto-Discovery configuration generator.
//!
//! Generates retained discovery messages under `<ha_discovery_prefix>/<component>/iopool_<pool_id>_<metric>/config`
//! so that each pool's sensors and binary sensors automatically appear in Home Assistant grouped under a single device.

use serde::Serialize;

use crate::error::Result;
use crate::models::{MqttMessage, Pool};

/// Home Assistant device metadata shared across all entities belonging to a single pool.
#[derive(Debug, Clone, Serialize)]
pub struct HaDevice {
    /// Unique identifiers for the device in Home Assistant.
    pub identifiers: Vec<String>,
    /// Human-readable device name in Home Assistant.
    pub name: String,
    /// Device manufacturer.
    pub manufacturer: &'static str,
    /// Device model name.
    pub model: &'static str,
    /// Optional hardware/probe identifier (`ecoId`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    /// Optional gateway identifier (`gatewayCode`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_device: Option<String>,
}

/// Home Assistant MQTT discovery payload for `sensor` and `binary_sensor` entities.
#[derive(Debug, Clone, Serialize)]
pub struct HaDiscoveryPayload {
    /// Display name of the entity within the device.
    pub name: String,
    /// Globally unique ID for the entity in Home Assistant.
    pub unique_id: String,
    /// Suggested object ID in Home Assistant.
    pub object_id: String,
    /// MQTT state topic containing the full pool JSON state (`<base_topic>/<pool_id>/state`).
    pub state_topic: String,
    /// Jinja2 template extracting the specific value from the JSON state payload.
    pub value_template: String,
    /// MQTT availability topic (`<base_topic>/bridge/state`).
    pub availability_topic: String,
    /// Payload indicating the bridge is online.
    pub payload_available: &'static str,
    /// Payload indicating the bridge is offline.
    pub payload_not_available: &'static str,
    /// Optional Home Assistant `device_class` (e.g. `temperature`, `ph`, `voltage`, `duration`, `timestamp`, `problem`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_class: Option<&'static str>,
    /// Optional Home Assistant `state_class` (e.g. `measurement`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_class: Option<&'static str>,
    /// Optional unit of measurement (e.g. `°C`, `mV`, `h`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_of_measurement: Option<&'static str>,
    /// Optional MDI icon (e.g. `mdi:pool`, `mdi:ph`, `mdi:flash-triangle`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<&'static str>,
    /// Optional entity category (`diagnostic` for metadata entities).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_category: Option<&'static str>,
    /// For `binary_sensor`, the value representing `ON`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_on: Option<&'static str>,
    /// For `binary_sensor`, the value representing `OFF`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_off: Option<&'static str>,
    /// Associated Home Assistant device block.
    pub device: HaDevice,
}

struct EntitySpec {
    component: &'static str,
    key: &'static str,
    name: &'static str,
    value_template: &'static str,
    device_class: Option<&'static str>,
    state_class: Option<&'static str>,
    unit: Option<&'static str>,
    icon: Option<&'static str>,
    entity_category: Option<&'static str>,
    payload_on: Option<&'static str>,
    payload_off: Option<&'static str>,
}

const ENTITY_SPECS: &[EntitySpec] = &[
    EntitySpec {
        component: "sensor",
        key: "temperature",
        name: "Temperature",
        value_template: "{{ value_json.latestMeasure.temperature | round(2) if value_json.latestMeasure is defined and value_json.latestMeasure.temperature is not none else none }}",
        device_class: Some("temperature"),
        state_class: Some("measurement"),
        unit: Some("°C"),
        icon: Some("mdi:pool-thermometer"),
        entity_category: None,
        payload_on: None,
        payload_off: None,
    },
    EntitySpec {
        component: "sensor",
        key: "ph",
        name: "pH",
        value_template: "{{ value_json.latestMeasure.ph | round(2) if value_json.latestMeasure is defined and value_json.latestMeasure.ph is not none else none }}",
        device_class: Some("ph"),
        state_class: Some("measurement"),
        unit: None,
        icon: Some("mdi:ph"),
        entity_category: None,
        payload_on: None,
        payload_off: None,
    },
    EntitySpec {
        component: "sensor",
        key: "orp",
        name: "ORP / Redox",
        value_template: "{{ value_json.latestMeasure.orp | int if value_json.latestMeasure is defined and value_json.latestMeasure.orp is not none else none }}",
        device_class: Some("voltage"),
        state_class: Some("measurement"),
        unit: Some("mV"),
        icon: Some("mdi:flash-triangle"),
        entity_category: None,
        payload_on: None,
        payload_off: None,
    },
    EntitySpec {
        component: "sensor",
        key: "filtration_duration",
        name: "Recommended Filtration Duration",
        value_template: "{{ value_json.advice.filtrationDuration if value_json.advice is defined and value_json.advice.filtrationDuration is not none else none }}",
        device_class: Some("duration"),
        state_class: Some("measurement"),
        unit: Some("h"),
        icon: Some("mdi:timer-outline"),
        entity_category: None,
        payload_on: None,
        payload_off: None,
    },
    EntitySpec {
        component: "sensor",
        key: "mode",
        name: "Pool Mode",
        value_template: "{{ value_json.mode }}",
        device_class: None,
        state_class: None,
        unit: None,
        icon: Some("mdi:pool"),
        entity_category: None,
        payload_on: None,
        payload_off: None,
    },
    EntitySpec {
        component: "sensor",
        key: "measure_mode",
        name: "Measure Mode",
        value_template: "{{ value_json.latestMeasure.mode if value_json.latestMeasure is defined else none }}",
        device_class: None,
        state_class: None,
        unit: None,
        icon: Some("mdi:access-point-network"),
        entity_category: Some("diagnostic"),
        payload_on: None,
        payload_off: None,
    },
    EntitySpec {
        component: "sensor",
        key: "measured_at",
        name: "Last Measured At",
        value_template: "{{ value_json.latestMeasure.measuredAt if value_json.latestMeasure is defined else none }}",
        device_class: Some("timestamp"),
        state_class: None,
        unit: None,
        icon: Some("mdi:clock-check-outline"),
        entity_category: Some("diagnostic"),
        payload_on: None,
        payload_off: None,
    },
    EntitySpec {
        component: "sensor",
        key: "eco_id",
        name: "EcO Probe ID",
        value_template: "{{ value_json.latestMeasure.ecoId if value_json.latestMeasure is defined else none }}",
        device_class: None,
        state_class: None,
        unit: None,
        icon: Some("mdi:identifier"),
        entity_category: Some("diagnostic"),
        payload_on: None,
        payload_off: None,
    },
    EntitySpec {
        component: "sensor",
        key: "gateway_code",
        name: "Gateway Code",
        value_template: "{{ value_json.latestMeasure.gatewayCode if value_json.latestMeasure is defined else none }}",
        device_class: None,
        state_class: None,
        unit: None,
        icon: Some("mdi:router-wireless"),
        entity_category: Some("diagnostic"),
        payload_on: None,
        payload_off: None,
    },
    EntitySpec {
        component: "binary_sensor",
        key: "action_required",
        name: "Action Required",
        value_template: "{{ 'ON' if value_json.hasAnActionRequired else 'OFF' }}",
        device_class: Some("problem"),
        state_class: None,
        unit: None,
        icon: Some("mdi:alert-circle-outline"),
        entity_category: None,
        payload_on: Some("ON"),
        payload_off: Some("OFF"),
    },
    EntitySpec {
        component: "binary_sensor",
        key: "measure_valid",
        name: "Measure Valid",
        value_template: "{{ 'ON' if value_json.latestMeasure is defined and value_json.latestMeasure.isValid else 'OFF' }}",
        device_class: None,
        state_class: None,
        unit: None,
        icon: Some("mdi:check-decagram"),
        entity_category: Some("diagnostic"),
        payload_on: Some("ON"),
        payload_off: Some("OFF"),
    },
];

/// Sanitizes a pool identifier so it is safe to use in Home Assistant `unique_id` and topic node IDs.
#[must_use]
pub fn sanitize_id(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Builds all Home Assistant MQTT Auto-Discovery messages for a given [`Pool`].
///
/// When `use_pool_title` is `true`:
/// - `state_topic` uses `<mqtt_base_topic>/<pool_title>/state`
/// - Each sensor's display `name` is prefixed with the pool title (e.g. `"Piscine Temperature"`)
/// - Each sensor's `object_id` and discovery topic use `iopool_<pool_title>_<metric>`
///
/// Discovery messages are always marked with `retain = true` so Home Assistant discovers
/// the entities even after a Home Assistant restart.
///
/// # Errors
///
/// Returns [`crate::error::Error::Json`] if serialization of a discovery payload fails.
pub fn build_ha_discovery_messages(
    pool: &Pool,
    mqtt_base_topic: &str,
    ha_discovery_prefix: &str,
    use_pool_title: bool,
) -> Result<Vec<MqttMessage>> {
    let topic_segment = pool.topic_segment(use_pool_title);
    let safe_node_id = sanitize_id(&topic_segment);
    let safe_pool_id = sanitize_id(&pool.id);
    let state_topic = format!("{mqtt_base_topic}/{topic_segment}/state");
    let availability_topic = format!("{mqtt_base_topic}/bridge/state");

    let eco_id = pool.latest_measure.as_ref().and_then(|m| m.eco_id.clone());
    let gateway_code = pool
        .latest_measure
        .as_ref()
        .and_then(|m| m.gateway_code.clone());

    let device = HaDevice {
        identifiers: vec![format!("iopool_{safe_pool_id}")],
        name: format!("iopool {}", pool.title),
        manufacturer: "iopool",
        model: "EcO Smart Pool Monitor",
        serial_number: eco_id,
        via_device: gateway_code,
    };

    let mut messages = Vec::with_capacity(ENTITY_SPECS.len());
    let trimmed_title = pool.title.trim();

    for spec in ENTITY_SPECS {
        let unique_id = format!("iopool_{safe_pool_id}_{}", spec.key);
        let object_id = format!("iopool_{safe_node_id}_{}", spec.key);
        let discovery_topic = format!(
            "{ha_discovery_prefix}/{}/{object_id}/config",
            spec.component
        );

        let entity_name = if use_pool_title && !trimmed_title.is_empty() {
            format!("{trimmed_title} {}", spec.name)
        } else {
            spec.name.to_owned()
        };

        let payload_struct = HaDiscoveryPayload {
            name: entity_name,
            unique_id,
            object_id,
            state_topic: state_topic.clone(),
            value_template: spec.value_template.to_owned(),
            availability_topic: availability_topic.clone(),
            payload_available: "online",
            payload_not_available: "offline",
            device_class: spec.device_class,
            state_class: spec.state_class,
            unit_of_measurement: spec.unit,
            icon: spec.icon,
            entity_category: spec.entity_category,
            payload_on: spec.payload_on,
            payload_off: spec.payload_off,
            device: device.clone(),
        };

        let payload = serde_json::to_string(&payload_struct)?;
        messages.push(MqttMessage::new(discovery_topic, payload, true));
    }

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_build_ha_discovery_messages() {
        let raw = r#"{
            "id": "cfd5a936-e8e6-4ad3-b985-512bee0d9ad9",
            "title": "Piscine",
            "latestMeasure": {
                "temperature": 24.5,
                "ph": 7.2,
                "orp": 680,
                "mode": "gateway",
                "isValid": true,
                "ecoId": "0C6rTwUH",
                "gatewayCode": "I20-98cdaceb5d60",
                "measuredAt": "2026-10-23T09:08:00.000Z"
            },
            "mode": "STANDARD",
            "hasAnActionRequired": false,
            "advice": { "filtrationDuration": 4 }
        }"#;
        let pool: Pool = serde_json::from_str(raw).unwrap();
        let msgs =
            build_ha_discovery_messages(&pool, "iopool2mqtt", "homeassistant", false).unwrap();

        assert_eq!(msgs.len(), ENTITY_SPECS.len());
        assert!(msgs.iter().all(|m| m.retain));

        let temp_msg = msgs
            .iter()
            .find(|m| m.topic.ends_with("_temperature/config"))
            .unwrap();
        assert_eq!(
            temp_msg.topic,
            "homeassistant/sensor/iopool_cfd5a936-e8e6-4ad3-b985-512bee0d9ad9_temperature/config"
        );

        let parsed: Value = serde_json::from_str(&temp_msg.payload).unwrap();
        assert_eq!(parsed["name"], "Temperature");
        assert_eq!(
            parsed["object_id"],
            "iopool_cfd5a936-e8e6-4ad3-b985-512bee0d9ad9_temperature"
        );
        assert_eq!(parsed["device_class"], "temperature");
        assert_eq!(parsed["unit_of_measurement"], "°C");
        assert_eq!(
            parsed["state_topic"],
            "iopool2mqtt/cfd5a936-e8e6-4ad3-b985-512bee0d9ad9/state"
        );
        assert_eq!(parsed["availability_topic"], "iopool2mqtt/bridge/state");
        assert_eq!(parsed["device"]["name"], "iopool Piscine");
        assert_eq!(parsed["device"]["serial_number"], "0C6rTwUH");
    }

    #[test]
    fn test_build_ha_discovery_messages_with_pool_title_enabled() {
        let raw = r#"{
            "id": "cfd5a936-e8e6-4ad3-b985-512bee0d9ad9",
            "title": "Piscine",
            "latestMeasure": {
                "temperature": 24.5,
                "ph": 7.2,
                "orp": 680,
                "mode": "gateway",
                "isValid": true,
                "ecoId": "0C6rTwUH",
                "gatewayCode": "I20-98cdaceb5d60",
                "measuredAt": "2026-10-23T09:08:00.000Z"
            },
            "mode": "STANDARD",
            "hasAnActionRequired": false,
            "advice": { "filtrationDuration": 4 }
        }"#;
        let pool: Pool = serde_json::from_str(raw).unwrap();
        let msgs =
            build_ha_discovery_messages(&pool, "iopool2mqtt", "homeassistant", true).unwrap();

        let temp_msg = msgs
            .iter()
            .find(|m| m.topic.ends_with("_temperature/config"))
            .unwrap();
        assert_eq!(
            temp_msg.topic,
            "homeassistant/sensor/iopool_Piscine_temperature/config"
        );

        let parsed: Value = serde_json::from_str(&temp_msg.payload).unwrap();
        assert_eq!(parsed["name"], "Piscine Temperature");
        assert_eq!(parsed["object_id"], "iopool_Piscine_temperature");
        assert_eq!(
            parsed["unique_id"],
            "iopool_cfd5a936-e8e6-4ad3-b985-512bee0d9ad9_temperature"
        );
        assert_eq!(parsed["state_topic"], "iopool2mqtt/Piscine/state");
    }
}
